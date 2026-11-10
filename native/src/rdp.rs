//! Embedded RDP console running on the workspace Tokio runtime.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use ironrdp::client::config::{ConfigBuilder, Destination};
use ironrdp::client::rdp::{RdpClient, RdpInputEvent, RdpOutputEvent};
use ironrdp::input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp::pdu::rdp::capability_sets::MajorPlatformType;
use tokio::sync::mpsc::{self, UnboundedSender, unbounded_channel};

use crate::worker::Event;

#[derive(Clone, Debug)]
pub enum RdpInput {
    Key {
        key: egui::Key,
        pressed: bool,
        ctrl: bool,
        alt: bool,
        shift: bool,
    },
    Text(String),
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
    Wheel {
        vertical: bool,
        units: i16,
    },
    CtrlAltDel,
    Close,
}

#[derive(Default)]
struct RdpRegistry {
    channels: HashMap<String, UnboundedSender<RdpInput>>,
    cancelled: HashSet<String>,
}

#[derive(Clone, Default)]
pub struct RdpHandles(Arc<Mutex<RdpRegistry>>);

impl RdpHandles {
    fn register(&self, console_id: &str, sender: UnboundedSender<RdpInput>) -> bool {
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

    fn send(&self, console_id: &str, input: RdpInput) {
        let Ok(mut registry) = self.0.lock() else {
            return;
        };
        if let Some(sender) = registry.channels.get(console_id) {
            if sender.send(input).is_err() {
                registry.channels.remove(console_id);
            }
            return;
        }
        if matches!(input, RdpInput::Close) {
            registry.cancelled.insert(console_id.to_owned());
        }
    }
}

pub fn send_input(handles: &RdpHandles, console_id: &str, input: RdpInput) {
    handles.send(console_id, input);
}

pub fn close(handles: &RdpHandles, console_id: &str) {
    handles.send(console_id, RdpInput::Close);
}

pub fn validate_credentials(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty() || host.chars().any(char::is_whitespace) || host.chars().count() > 255 {
        return Err("Enter the guest host name or IP address.".into());
    }
    if port == 0 {
        return Err("RDP port must be between 1 and 65535.".into());
    }
    if username.trim().is_empty() || username.chars().count() > 255 {
        return Err("Enter the RDP user name.".into());
    }
    if password.is_empty() || password.chars().count() > 1024 {
        return Err("Enter the RDP password.".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn open(
    events: UnboundedSender<Event>,
    handles: RdpHandles,
    console_id: String,
    host: String,
    port: u16,
    username: String,
    domain: String,
    password: String,
    width: u16,
    height: u16,
) -> Result<(), String> {
    validate_credentials(&host, port, &username, &password)?;
    let destination = Destination::new(format!("{host}:{port}"))
        .map_err(|error| format!("Invalid RDP destination: {error}"))?;
    let mut builder = ConfigBuilder::new()
        .with_destination(destination)
        .with_username(username.clone())
        .with_password(password)
        .with_client_build(1)
        .with_client_dir("C:\\Windows\\System32\\mstscax.dll")
        .with_platform(MajorPlatformType::UNIX)
        .with_client_name("xen-desktop")
        .with_desktop_width(width.max(640))
        .with_desktop_height(height.max(480))
        .with_desktop_scale_factor(100)
        .with_credssp(true);
    if !domain.trim().is_empty() {
        builder = builder.with_domain(domain.trim());
    }
    let config = builder
        .build()
        .map_err(|error| format!("Cannot build the RDP configuration: {error}"))?;
    let (output_tx, mut output_rx) = mpsc::channel::<RdpOutputEvent>(8);
    let client = RdpClient::new(config, output_tx);
    let input_sender = client.input_sender();
    let (input_tx, mut input_rx) = unbounded_channel::<RdpInput>();
    if !handles.register(&console_id, input_tx) {
        return Ok(());
    }
    let _ = events.send(Event::RdpOpened {
        console_id: console_id.clone(),
        host,
        username,
    });
    let task_events = events.clone();
    let task_handles = handles.clone();
    let task_id = console_id.clone();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async move {
            let local = tokio::task::LocalSet::new();
            local.run_until(async move {
        let client_task = tokio::task::spawn_local(client.run());
        let mut database = Database::new();
        let mut message = None;
        loop {
            tokio::select! {
                output = output_rx.recv() => {
                    match output {
                        Some(RdpOutputEvent::Image { buffer, width, height }) => {
                            let _ = task_events.send(Event::RdpFrame {
                                console_id: task_id.clone(),
                                width: width.get(),
                                height: height.get(),
                                pixels: buffer,
                            });
                        }
                        Some(RdpOutputEvent::ConnectionFailure(error)) => {
                            message = Some(format!("RDP connection failed: {error}"));
                            break;
                        }
                        Some(RdpOutputEvent::Terminated(result)) => {
                            message = Some(match result {
                                Ok(_) => "RDP session ended.".to_owned(),
                                Err(error) => format!("RDP session ended: {error}"),
                            });
                            break;
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
                input = input_rx.recv() => {
                    match input {
                        Some(RdpInput::Close) | None => {
                            let _ = input_sender.send(RdpInputEvent::Close);
                        }
                        Some(input) => {
                            let events = translate_input(input, &mut database);
                            if !events.is_empty() {
                                let _ = input_sender.send(RdpInputEvent::FastPath(events.into()));
                            }
                        }
                    }
                }
            }
        }
        let _ = input_sender.send(RdpInputEvent::Close);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), client_task).await;
        task_handles.remove(&task_id);
        let _ = task_events.send(Event::RdpClosed {
            console_id: task_id,
            message,
        });
        }).await;
        });
    });
    Ok(())
}

fn translate_input(
    input: RdpInput,
    database: &mut Database,
) -> Vec<ironrdp::pdu::input::fast_path::FastPathInputEvent> {
    let mut operations = Vec::new();
    match input {
        RdpInput::Key {
            key,
            pressed,
            ctrl,
            alt,
            shift,
        } => {
            if ctrl {
                operations.push(Operation::KeyPressed(scancode(false, 0x1D)));
            }
            if alt {
                operations.push(Operation::KeyPressed(scancode(false, 0x38)));
            }
            if shift {
                operations.push(Operation::KeyPressed(scancode(false, 0x2A)));
            }
            if let Some((code, extended)) = key_scancode(key) {
                let key = scancode(extended, code);
                operations.push(if pressed {
                    Operation::KeyPressed(key)
                } else {
                    Operation::KeyReleased(key)
                });
            }
            if shift {
                operations.push(Operation::KeyReleased(scancode(false, 0x2A)));
            }
            if alt {
                operations.push(Operation::KeyReleased(scancode(false, 0x38)));
            }
            if ctrl {
                operations.push(Operation::KeyReleased(scancode(false, 0x1D)));
            }
        }
        RdpInput::Text(text) => {
            for character in text.chars() {
                operations.push(Operation::UnicodeKeyPressed(character));
                operations.push(Operation::UnicodeKeyReleased(character));
            }
        }
        RdpInput::MouseMove { x, y } => {
            operations.push(Operation::MouseMove(MousePosition { x, y }));
        }
        RdpInput::MouseButton { button, pressed } => {
            let button = match button {
                0 => MouseButton::Left,
                1 => MouseButton::Middle,
                _ => MouseButton::Right,
            };
            operations.push(if pressed {
                Operation::MouseButtonPressed(button)
            } else {
                Operation::MouseButtonReleased(button)
            });
        }
        RdpInput::Wheel { vertical, units } => {
            operations.push(Operation::WheelRotations(WheelRotations {
                is_vertical: vertical,
                rotation_units: units,
            }));
        }
        RdpInput::CtrlAltDel => {
            operations.push(Operation::KeyPressed(scancode(false, 0x1D)));
            operations.push(Operation::KeyPressed(scancode(false, 0x38)));
            operations.push(Operation::KeyPressed(scancode(true, 0x4F)));
            operations.push(Operation::KeyReleased(scancode(true, 0x4F)));
            operations.push(Operation::KeyReleased(scancode(false, 0x38)));
            operations.push(Operation::KeyReleased(scancode(false, 0x1D)));
        }
        RdpInput::Close => {}
    }
    database.apply(operations).into_iter().collect()
}

fn scancode(extended: bool, code: u8) -> Scancode {
    Scancode::from_u8(extended, code)
}

fn key_scancode(key: egui::Key) -> Option<(u8, bool)> {
    use egui::Key;
    Some(match key {
        Key::A => (0x1E, false),
        Key::B => (0x30, false),
        Key::C => (0x2E, false),
        Key::D => (0x20, false),
        Key::E => (0x12, false),
        Key::F => (0x21, false),
        Key::G => (0x22, false),
        Key::H => (0x23, false),
        Key::I => (0x17, false),
        Key::J => (0x24, false),
        Key::K => (0x25, false),
        Key::L => (0x26, false),
        Key::M => (0x32, false),
        Key::N => (0x31, false),
        Key::O => (0x18, false),
        Key::P => (0x19, false),
        Key::Q => (0x10, false),
        Key::R => (0x13, false),
        Key::S => (0x1F, false),
        Key::T => (0x14, false),
        Key::U => (0x16, false),
        Key::V => (0x2F, false),
        Key::W => (0x11, false),
        Key::X => (0x2D, false),
        Key::Y => (0x15, false),
        Key::Z => (0x2C, false),
        Key::Num0 => (0x0B, false),
        Key::Num1 => (0x02, false),
        Key::Num2 => (0x03, false),
        Key::Num3 => (0x04, false),
        Key::Num4 => (0x05, false),
        Key::Num5 => (0x06, false),
        Key::Num6 => (0x07, false),
        Key::Num7 => (0x08, false),
        Key::Num8 => (0x09, false),
        Key::Num9 => (0x0A, false),
        Key::Space => (0x39, false),
        Key::Enter => (0x1C, false),
        Key::Tab => (0x0F, false),
        Key::Backspace => (0x0E, false),
        Key::Escape => (0x01, false),
        Key::Minus => (0x0C, false),
        Key::Equals | Key::Plus => (0x0D, false),
        Key::OpenBracket | Key::OpenCurlyBracket => (0x1A, false),
        Key::CloseBracket | Key::CloseCurlyBracket => (0x1B, false),
        Key::Backslash | Key::Pipe => (0x2B, false),
        Key::Semicolon | Key::Colon => (0x27, false),
        Key::Quote => (0x28, false),
        Key::Comma => (0x33, false),
        Key::Period => (0x34, false),
        Key::Slash | Key::Questionmark => (0x35, false),
        Key::Backtick => (0x29, false),
        Key::ArrowUp => (0x48, true),
        Key::ArrowDown => (0x50, true),
        Key::ArrowLeft => (0x4B, true),
        Key::ArrowRight => (0x4D, true),
        Key::Home => (0x47, true),
        Key::End => (0x4F, true),
        Key::PageUp => (0x49, true),
        Key::PageDown => (0x51, true),
        Key::Insert => (0x52, true),
        Key::Delete => (0x53, true),
        Key::F1 => (0x3B, false),
        Key::F2 => (0x3C, false),
        Key::F3 => (0x3D, false),
        Key::F4 => (0x3E, false),
        Key::F5 => (0x3F, false),
        Key::F6 => (0x40, false),
        Key::F7 => (0x41, false),
        Key::F8 => (0x42, false),
        Key::F9 => (0x43, false),
        Key::F10 => (0x44, false),
        Key::F11 => (0x57, false),
        Key::F12 => (0x58, false),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdp_credentials_are_validated() {
        assert!(validate_credentials("192.168.8.10", 3389, "Administrator", "secret").is_ok());
        assert!(validate_credentials("", 3389, "Administrator", "secret").is_err());
        assert!(validate_credentials("bad host", 3389, "Administrator", "secret").is_err());
        assert!(validate_credentials("host", 0, "Administrator", "secret").is_err());
        assert!(validate_credentials("host", 3389, "", "secret").is_err());
        assert!(validate_credentials("host", 3389, "Administrator", "").is_err());
    }

    #[test]
    fn rdp_key_map_covers_letters_and_extended_keys() {
        assert_eq!(key_scancode(egui::Key::A), Some((0x1E, false)));
        assert_eq!(key_scancode(egui::Key::Enter), Some((0x1C, false)));
        assert_eq!(key_scancode(egui::Key::ArrowUp), Some((0x48, true)));
        assert_eq!(key_scancode(egui::Key::Delete), Some((0x53, true)));
        assert_eq!(key_scancode(egui::Key::MediaPlayPause), None);
    }

    #[test]
    fn input_translation_emits_fast_path_events() {
        let mut database = Database::new();
        let events = translate_input(RdpInput::Text("hi".into()), &mut database);
        assert!(!events.is_empty());
        let events = translate_input(RdpInput::MouseMove { x: 10, y: 20 }, &mut database);
        assert_eq!(events.len(), 1);
        let events = translate_input(RdpInput::CtrlAltDel, &mut database);
        assert!(!events.is_empty());
    }
}
