use std::{path::Path, sync::Arc, thread::JoinHandle, time::Duration};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use xen_core::{
    host::HostDetails,
    operations::VmAction,
    performance::HostPerformance,
    vm::VmDetails,
    workspace::{Workspace, WorkspaceView},
    xapi::ConnectionSettings,
};

/// Commands sent from the UI thread to the background workspace thread.
pub enum Command {
    Connect {
        settings: ConnectionSettings,
        password: String,
        profile_id: Option<String>,
    },
    Refresh {
        connection_id: String,
    },
    Disconnect {
        connection_id: String,
    },
    Remove {
        connection_id: String,
    },
    Action {
        connection_id: String,
        vm_uuid: String,
        action: VmAction,
    },
    CheckTask {
        task_id: String,
    },
    CancelTask {
        task_id: String,
    },
    HostDetails {
        seq: u64,
        connection_id: String,
        host_uuid: String,
    },
    HostPerformance {
        seq: u64,
        connection_id: String,
        host_uuid: String,
        range_seconds: u64,
    },
    VmDetails {
        seq: u64,
        connection_id: String,
        vm_uuid: String,
    },
    VmPerformance {
        seq: u64,
        connection_id: String,
        vm_uuid: String,
        range_seconds: u64,
    },
    EditVm {
        connection_id: String,
        vm_uuid: String,
        name: String,
        description: String,
        vcpus_max: u64,
        vcpus_at_startup: u64,
        memory_static_max_mib: u64,
        memory_dynamic_min_mib: u64,
        memory_dynamic_max_mib: u64,
        boot_order: Option<String>,
    },
    OpenConsole {
        console_id: String,
        connection_id: String,
        vm_uuid: String,
    },
    ConsoleInput {
        console_id: String,
        input: crate::console::ConsoleInput,
    },
    CloseConsole {
        console_id: String,
    },
    Shutdown,
}

/// Results sent back to the UI thread.
pub enum Event {
    Workspace(WorkspaceView),
    Connected(String),
    ActionQueued,
    RefreshFinished {
        connection_id: String,
        result: Result<(), String>,
    },
    TaskFinished {
        task_id: String,
        result: Result<(), String>,
    },
    ConnectionClosed {
        connection_id: String,
        result: Result<(), String>,
    },
    ConnectionRemoved {
        connection_id: String,
        result: Result<(), String>,
    },
    HostDetails {
        seq: u64,
        result: Result<Box<HostDetails>, String>,
    },
    HostPerformance {
        seq: u64,
        result: Result<Box<HostPerformance>, String>,
    },
    VmDetails {
        seq: u64,
        result: Result<Box<VmDetails>, String>,
    },
    VmPerformance {
        seq: u64,
        result: Result<Box<HostPerformance>, String>,
    },
    VmEdited {
        vm_uuid: String,
        result: Result<(), String>,
    },
    ConsoleOpened {
        console_id: String,
        vm_uuid: String,
        console_uuid: String,
    },
    ConsoleSize {
        console_id: String,
        width: u16,
        height: u16,
    },
    ConsoleFrame {
        console_id: String,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
    },
    ConsoleCopy {
        console_id: String,
        dst: (u16, u16, u16, u16),
        src: (u16, u16, u16, u16),
    },
    ConsoleClipboard {
        console_id: String,
        text: String,
    },
    ConsoleClosed {
        console_id: String,
        message: Option<String>,
    },
    Failed {
        context: String,
        message: String,
    },
}

pub struct Worker {
    commands: UnboundedSender<Command>,
    pub events: UnboundedReceiver<Event>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start() -> Self {
        let (commands, command_rx) = unbounded_channel::<Command>();
        let (event_tx, events) = unbounded_channel::<Event>();
        let join = std::thread::Builder::new()
            .name("xen-workspace".into())
            .spawn(move || run_worker(command_rx, event_tx))
            .expect("start workspace thread");
        Self {
            commands,
            events,
            join: Some(join),
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(mut commands: UnboundedReceiver<Command>, events: UnboundedSender<Event>) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    runtime.block_on(async move {
        let path = crate::config::profiles_path();
        let profiles = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let workspace = Workspace::with_profiles(profiles);
        let consoles = crate::console::ConsoleHandles::default();
        let _ = events.send(Event::Workspace(workspace.view().await));

        let mut tick = tokio::time::interval(Duration::from_millis(1_000));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    if matches!(command, Command::Shutdown) {
                        workspace.close().await;
                        break;
                    }
                    dispatch(&workspace, &events, &path, &consoles, command);
                }
                _ = tick.tick() => {
                    let _ = events.send(Event::Workspace(workspace.view().await));
                }
            }
        }
    });
}

fn dispatch(
    workspace: &Arc<Workspace>,
    events: &UnboundedSender<Event>,
    path: &Path,
    consoles: &crate::console::ConsoleHandles,
    command: Command,
) {
    let workspace = workspace.clone();
    let events = events.clone();
    let path = path.to_path_buf();
    let consoles = consoles.clone();

    tokio::spawn(async move {
        match command {
            Command::Connect {
                settings,
                password,
                profile_id,
            } => match workspace.connect(settings, &password, profile_id).await {
                Ok(id) => {
                    save_profiles(&workspace, &path).await;
                    let _ = events.send(Event::Connected(id));
                }
                Err(error) => {
                    let _ = events.send(Event::Failed {
                        context: "connect".into(),
                        message: error,
                    });
                }
            },
            Command::Refresh { connection_id } => {
                let result = workspace.refresh(&connection_id).await;
                let _ = events.send(Event::RefreshFinished {
                    connection_id,
                    result,
                });
            }
            Command::Disconnect { connection_id } => {
                let result = workspace.disconnect(&connection_id).await;
                let _ = events.send(Event::ConnectionClosed {
                    connection_id,
                    result,
                });
            }
            Command::Remove { connection_id } => {
                let result = workspace.remove(&connection_id).await;
                if result.is_ok() {
                    save_profiles(&workspace, &path).await;
                }
                let _ = events.send(Event::ConnectionRemoved {
                    connection_id,
                    result,
                });
            }
            Command::Action {
                connection_id,
                vm_uuid,
                action,
            } => match workspace.start_action(connection_id, vm_uuid, action).await {
                Ok(_) => {
                    let _ = events.send(Event::ActionQueued);
                }
                Err(error) => {
                    let _ = events.send(Event::Failed {
                        context: "action".into(),
                        message: error,
                    });
                }
            },
            Command::CheckTask { task_id } => {
                let result = workspace.check_task(&task_id).await;
                let _ = events.send(Event::TaskFinished { task_id, result });
            }
            Command::CancelTask { task_id } => {
                let result = workspace.cancel_task(&task_id).await;
                let _ = events.send(Event::TaskFinished { task_id, result });
            }
            Command::HostDetails {
                seq,
                connection_id,
                host_uuid,
            } => {
                let result = workspace
                    .host_details(&connection_id, &host_uuid)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::HostDetails { seq, result });
            }
            Command::HostPerformance {
                seq,
                connection_id,
                host_uuid,
                range_seconds,
            } => {
                let result = workspace
                    .host_performance(&connection_id, &host_uuid, range_seconds)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::HostPerformance { seq, result });
            }
            Command::VmDetails {
                seq,
                connection_id,
                vm_uuid,
            } => {
                let result = workspace
                    .vm_details(&connection_id, &vm_uuid)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::VmDetails { seq, result });
            }
            Command::VmPerformance {
                seq,
                connection_id,
                vm_uuid,
                range_seconds,
            } => {
                let result = workspace
                    .vm_performance(&connection_id, &vm_uuid, range_seconds)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::VmPerformance { seq, result });
            }
            Command::EditVm {
                connection_id,
                vm_uuid,
                name,
                description,
                vcpus_max,
                vcpus_at_startup,
                memory_static_max_mib,
                memory_dynamic_min_mib,
                memory_dynamic_max_mib,
                boot_order,
            } => {
                let result = workspace
                    .update_vm(
                        &connection_id,
                        &vm_uuid,
                        name,
                        description,
                        vcpus_max,
                        vcpus_at_startup,
                        memory_static_max_mib,
                        memory_dynamic_min_mib,
                        memory_dynamic_max_mib,
                        boot_order,
                    )
                    .await;
                let _ = events.send(Event::VmEdited { vm_uuid, result });
            }
            Command::OpenConsole {
                console_id,
                connection_id,
                vm_uuid,
            } => {
                if let Err(message) = crate::console::open(
                    workspace.clone(),
                    events.clone(),
                    consoles,
                    console_id.clone(),
                    connection_id,
                    vm_uuid,
                )
                .await
                {
                    let _ = events.send(Event::ConsoleClosed {
                        console_id,
                        message: Some(message),
                    });
                }
            }
            Command::ConsoleInput { console_id, input } => {
                crate::console::send_input(&consoles, &console_id, input);
            }
            Command::CloseConsole { console_id } => {
                crate::console::close(&consoles, &console_id);
            }
            Command::Shutdown => {}
        }
        let _ = events.send(Event::Workspace(workspace.view().await));
    });
}

async fn save_profiles(workspace: &Workspace, path: &Path) {
    let profiles = workspace.profiles().await;
    let Ok(bytes) = serde_json::to_vec_pretty(&profiles) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}
