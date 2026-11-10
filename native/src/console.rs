//! VNC console session running on the workspace Tokio runtime.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vnc::{VncEncoding, VncEvent};

use crate::worker::Event;

#[derive(Clone, Debug)]
pub enum ConsoleInput {
    Key { keycode: u32, down: bool },
    Pointer { x: u16, y: u16, buttons: u8 },
    Close,
}

#[derive(Default)]
struct ConsoleRegistry {
    channels: HashMap<String, UnboundedSender<ConsoleInput>>,
    cancelled: HashSet<String>,
}

#[derive(Clone, Default)]
pub struct ConsoleHandles(Arc<Mutex<ConsoleRegistry>>);

impl ConsoleHandles {
    /// Returns false when the console was closed while the tunnel was opening.
    fn register(&self, console_id: &str, sender: UnboundedSender<ConsoleInput>) -> bool {
        let Ok(mut registry) = self.0.lock() else {
            return false;
        };
        if registry.cancelled.remove(console_id) {
            return false;
        }
        registry.channels.insert(console_id.to_owned(), sender);
        true
    }

    fn remove(&self, console_id: &str) {
        if let Ok(mut registry) = self.0.lock() {
            registry.channels.remove(console_id);
        }
    }

    fn send(&self, console_id: &str, input: ConsoleInput) {
        let Ok(mut registry) = self.0.lock() else {
            return;
        };
        if let Some(sender) = registry.channels.get(console_id) {
            if sender.send(input).is_err() {
                registry.channels.remove(console_id);
            }
            return;
        }
        if matches!(input, ConsoleInput::Close) {
            registry.cancelled.insert(console_id.to_owned());
        }
    }
}

pub fn send_input(handles: &ConsoleHandles, console_id: &str, input: ConsoleInput) {
    handles.send(console_id, input);
}

pub fn close(handles: &ConsoleHandles, console_id: &str) {
    handles.send(console_id, ConsoleInput::Close);
}

pub async fn open(
    workspace: Arc<xen_core::workspace::Workspace>,
    events: UnboundedSender<Event>,
    handles: ConsoleHandles,
    console_id: String,
    connection_id: String,
    vm_uuid: String,
) -> Result<(), String> {
    let tunnel = workspace
        .console_tunnel(&connection_id, &vm_uuid)
        .await
        .map_err(|error| format!("Cannot open the console tunnel: {error}"))?;
    let console_uuid = tunnel.console_uuid.clone();
    let client = vnc::VncConnector::new(tunnel.stream)
        .set_auth_method(async { Ok(String::new()) })
        .add_encoding(VncEncoding::CopyRect)
        .add_encoding(VncEncoding::Raw)
        .add_encoding(VncEncoding::DesktopSizePseudo)
        .allow_shared(true)
        .set_pixel_format(vnc::PixelFormat::rgba())
        .build()
        .map_err(|error| format!("Cannot create the VNC client: {error}"))?
        .try_start()
        .await
        .map_err(|error| format!("The VNC handshake failed: {error}"))?
        .finish()
        .map_err(|error| format!("The VNC handshake failed: {error}"))?;

    let (input_tx, mut input_rx) = unbounded_channel::<ConsoleInput>();
    if !handles.register(&console_id, input_tx) {
        let _ = client.close().await;
        return Ok(());
    }
    let _ = events.send(Event::ConsoleOpened {
        console_id: console_id.clone(),
        vm_uuid,
        console_uuid,
    });

    let task_events = events.clone();
    let task_handles = handles.clone();
    let task_id = console_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(200));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut closed = None;
        loop {
            tokio::select! {
                event = client.poll_event() => {
                    match event {
                        Ok(Some(VncEvent::SetResolution(screen))) => {
                            let _ = task_events.send(Event::ConsoleSize {
                                console_id: task_id.clone(),
                                width: screen.width,
                                height: screen.height,
                            });
                        }
                        Ok(Some(VncEvent::RawImage(rect, pixels))) => {
                            let _ = task_events.send(Event::ConsoleFrame {
                                console_id: task_id.clone(),
                                x: rect.x,
                                y: rect.y,
                                width: rect.width,
                                height: rect.height,
                                pixels,
                            });
                        }
                        Ok(Some(VncEvent::Copy(dst, src))) => {
                            let _ = task_events.send(Event::ConsoleCopy {
                                console_id: task_id.clone(),
                                dst: (dst.x, dst.y, dst.width, dst.height),
                                src: (src.x, src.y, src.width, src.height),
                            });
                        }
                        Ok(Some(VncEvent::Text(text))) => {
                            let _ = task_events.send(Event::ConsoleClipboard {
                                console_id: task_id.clone(),
                                text,
                            });
                        }
                        Ok(Some(VncEvent::Error(message))) => {
                            closed = Some(message);
                            break;
                        }
                        Ok(None) => {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                        Ok(Some(_)) => {}
                        Err(error) => {
                            closed = Some(error.to_string());
                            break;
                        }
                    }
                }
                input = input_rx.recv() => {
                    match input {
                        Some(ConsoleInput::Key { keycode, down }) => {
                            let _ = client.input(vnc::X11Event::KeyEvent((keycode, down).into())).await;
                        }
                        Some(ConsoleInput::Pointer { x, y, buttons }) => {
                            let _ = client.input(vnc::X11Event::PointerEvent((x, y, buttons).into())).await;
                        }
                        Some(ConsoleInput::Close) | None => break,
                    }
                }
                _ = ticker.tick() => {
                    let _ = client.input(vnc::X11Event::Refresh).await;
                }
            }
        }
        let _ = client.close().await;
        task_handles.remove(&task_id);
        let _ = task_events.send(Event::ConsoleClosed {
            console_id: task_id,
            message: closed,
        });
    });
    Ok(())
}
