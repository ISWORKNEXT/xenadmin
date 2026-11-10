#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use xen_core::{
    operations::VmAction,
    workspace::{Operation, Profile, Workspace, WorkspaceView},
    xapi::ConnectionSettings,
};

struct DesktopState {
    workspace: Arc<Workspace>,
    profiles_path: PathBuf,
    profile_writer: Mutex<()>,
}
impl DesktopState {
    async fn save(&self) -> Result<(), String> {
        let _writer = self.profile_writer.lock().await;
        let profiles = self.workspace.profiles().await;
        let bytes = serde_json::to_vec_pretty(&profiles)
            .map_err(|_| "Could not serialize connection profiles.")?;
        if let Some(parent) = self.profiles_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Could not create application settings directory.")?;
        }
        let temporary = self.profiles_path.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|_| "Could not save connection profiles.")?;
        std::fs::rename(temporary, &self.profiles_path)
            .map_err(|_| "Could not replace connection profiles.")?;
        Ok(())
    }
}

#[tauri::command]
async fn current_workspace(state: State<'_, Arc<DesktopState>>) -> Result<WorkspaceView, String> {
    Ok(state.workspace.view().await)
}
#[tauri::command]
async fn connect_host(
    settings: ConnectionSettings,
    password: String,
    profile_id: Option<String>,
    state: State<'_, Arc<DesktopState>>,
) -> Result<String, String> {
    let id = state
        .workspace
        .connect(settings, &password, profile_id)
        .await?;
    drop(password);
    state.save().await?;
    Ok(id)
}
#[tauri::command]
async fn refresh_connection(
    connection_id: String,
    state: State<'_, Arc<DesktopState>>,
) -> Result<(), String> {
    state.workspace.refresh(&connection_id).await
}
#[tauri::command]
async fn host_details(
    connection_id: String,
    host_uuid: String,
    state: State<'_, Arc<DesktopState>>,
) -> Result<xen_core::host::HostDetails, String> {
    state
        .workspace
        .host_details(&connection_id, &host_uuid)
        .await
}
#[tauri::command]
async fn host_performance(
    connection_id: String,
    host_uuid: String,
    range_seconds: u64,
    state: State<'_, Arc<DesktopState>>,
) -> Result<xen_core::performance::HostPerformance, String> {
    state
        .workspace
        .host_performance(&connection_id, &host_uuid, range_seconds)
        .await
}
#[tauri::command]
async fn disconnect_host(
    connection_id: String,
    state: State<'_, Arc<DesktopState>>,
) -> Result<(), String> {
    state.workspace.disconnect(&connection_id).await
}
#[tauri::command]
async fn remove_connection(
    connection_id: String,
    state: State<'_, Arc<DesktopState>>,
) -> Result<(), String> {
    state.workspace.remove(&connection_id).await?;
    state.save().await
}
#[tauri::command]
async fn start_vm_action(
    connection_id: String,
    vm_uuid: String,
    action: VmAction,
    state: State<'_, Arc<DesktopState>>,
) -> Result<Operation, String> {
    state
        .workspace
        .start_action(connection_id, vm_uuid, action)
        .await
}
#[tauri::command]
async fn check_task(task_id: String, state: State<'_, Arc<DesktopState>>) -> Result<(), String> {
    state.workspace.check_task(&task_id).await
}
#[tauri::command]
async fn cancel_task(task_id: String, state: State<'_, Arc<DesktopState>>) -> Result<(), String> {
    state.workspace.cancel_task(&task_id).await
}

#[derive(serde::Deserialize)]
struct Startup {
    settings: ConnectionSettings,
    password: String,
}

#[cfg(feature = "preview")]
mod preview {
    use super::*;
    use axum::{
        Json, Router,
        extract::State as AxumState,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use serde_json::{Value, json};
    #[derive(serde::Deserialize)]
    #[serde(tag = "command", content = "args", rename_all = "snake_case")]
    enum Command {
        CurrentWorkspace,
        ConnectHost {
            settings: ConnectionSettings,
            password: String,
            #[serde(rename = "profileId")]
            profile_id: Option<String>,
        },
        RefreshConnection {
            #[serde(rename = "connectionId")]
            connection_id: String,
        },
        HostDetails {
            #[serde(rename = "connectionId")]
            connection_id: String,
            #[serde(rename = "hostUuid")]
            host_uuid: String,
        },
        HostPerformance {
            #[serde(rename = "connectionId")]
            connection_id: String,
            #[serde(rename = "hostUuid")]
            host_uuid: String,
            #[serde(rename = "rangeSeconds")]
            range_seconds: u64,
        },
        DisconnectHost {
            #[serde(rename = "connectionId")]
            connection_id: String,
        },
        RemoveConnection {
            #[serde(rename = "connectionId")]
            connection_id: String,
        },
        StartVmAction {
            #[serde(rename = "connectionId")]
            connection_id: String,
            #[serde(rename = "vmUuid")]
            vm_uuid: String,
            action: VmAction,
        },
        CheckTask {
            #[serde(rename = "taskId")]
            task_id: String,
        },
        CancelTask {
            #[serde(rename = "taskId")]
            task_id: String,
        },
    }
    async fn command(
        AxumState((state, token)): AxumState<(Arc<DesktopState>, String)>,
        headers: HeaderMap,
        Json(command): Json<Command>,
    ) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
        if headers
            .get("x-xen-preview-token")
            .and_then(|v| v.to_str().ok())
            != Some(token.as_str())
        {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"Preview authorization required"})),
            ));
        }
        let result: Result<Value, String> = match command {
            Command::CurrentWorkspace => serde_json::to_value(state.workspace.view().await)
                .map_err(|_| "Could not encode workspace.".into()),
            Command::ConnectHost {
                settings,
                password,
                profile_id,
            } => match state
                .workspace
                .connect(settings, &password, profile_id)
                .await
            {
                Ok(id) => state.save().await.map(|_| json!(id)),
                Err(error) => Err(error),
            },
            Command::RefreshConnection { connection_id } => state
                .workspace
                .refresh(&connection_id)
                .await
                .map(|_| Value::Null),
            Command::HostDetails {
                connection_id,
                host_uuid,
            } => state
                .workspace
                .host_details(&connection_id, &host_uuid)
                .await
                .and_then(|details| {
                    serde_json::to_value(details)
                        .map_err(|_| "Could not encode host details.".into())
                }),
            Command::HostPerformance {
                connection_id,
                host_uuid,
                range_seconds,
            } => state
                .workspace
                .host_performance(&connection_id, &host_uuid, range_seconds)
                .await
                .and_then(|metrics| {
                    serde_json::to_value(metrics)
                        .map_err(|_| "Could not encode host metrics.".into())
                }),
            Command::DisconnectHost { connection_id } => state
                .workspace
                .disconnect(&connection_id)
                .await
                .map(|_| Value::Null),
            Command::RemoveConnection { connection_id } => {
                match state.workspace.remove(&connection_id).await {
                    Ok(()) => state.save().await.map(|_| Value::Null),
                    Err(error) => Err(error),
                }
            }
            Command::StartVmAction {
                connection_id,
                vm_uuid,
                action,
            } => state
                .workspace
                .start_action(connection_id, vm_uuid, action)
                .await
                .and_then(|op| {
                    serde_json::to_value(op).map_err(|_| "Could not encode task.".into())
                }),
            Command::CheckTask { task_id } => state
                .workspace
                .check_task(&task_id)
                .await
                .map(|_| Value::Null),
            Command::CancelTask { task_id } => state
                .workspace
                .cancel_task(&task_id)
                .await
                .map(|_| Value::Null),
        };
        result
            .map(Json)
            .map_err(|error| (StatusCode::BAD_REQUEST, Json(json!({"error":error}))))
    }
    pub async fn serve(state: Arc<DesktopState>, token: String) {
        let router = Router::new()
            .route("/command", post(command))
            .with_state((state, token));
        match tokio::net::TcpListener::bind("127.0.0.1:1421").await {
            Ok(listener) => {
                if axum::serve(listener, router).await.is_err() {
                    eprintln!("Preview server stopped.");
                }
            }
            Err(_) => eprintln!("Could not bind development preview port."),
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn host_detail_command_requires_both_connection_and_host() {
            let value = json!({"command":"host_details","args":{"connectionId":"pool-one","hostUuid":"host-one"}});
            let command: Command =
                serde_json::from_value(value).expect("valid host detail command");
            match command {
                Command::HostDetails {
                    connection_id,
                    host_uuid,
                } => {
                    assert_eq!(connection_id, "pool-one");
                    assert_eq!(host_uuid, "host-one");
                }
                _ => panic!("host detail request decoded as another operation"),
            }
            assert!(
                serde_json::from_value::<Command>(
                    json!({"command":"host_details","args":{"connectionId":"pool-one"}})
                )
                .is_err()
            );
        }

        #[test]
        fn renderer_cannot_submit_arbitrary_xenapi_methods() {
            assert!(
                serde_json::from_value::<Command>(
                    json!({"command":"raw_rpc","args":{"method":"host.reboot"}})
                )
                .is_err()
            );
        }
    }
}

fn main() {
    let startup = if std::env::args().any(|arg| arg == "--connect-stdin") {
        use std::io::BufRead;
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        let parsed = serde_json::from_str::<Vec<Startup>>(&line)
            .or_else(|_| serde_json::from_str::<Startup>(&line).map(|s| vec![s]));
        drop(line);
        parsed.unwrap_or_else(|_| {
            eprintln!("Invalid startup connection input.");
            Vec::new()
        })
    } else {
        Vec::new()
    };
    tauri::Builder::default()
        .setup(move |app| {
            let profiles_path = app.path().app_data_dir()?.join("connections.json");
            let profiles: Vec<Profile> = std::fs::read(&profiles_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let workspace = Workspace::with_profiles(profiles);
            tauri::async_runtime::block_on(async {
                for entry in startup {
                    if let Err(error) = workspace
                        .connect(entry.settings, &entry.password, None)
                        .await
                    {
                        eprintln!("Startup connection: {error}");
                    }
                }
            });
            let state = Arc::new(DesktopState {
                workspace,
                profiles_path,
                profile_writer: Mutex::new(()),
            });
            tauri::async_runtime::block_on(state.save())?;
            #[cfg(feature = "preview")]
            if let Ok(token) = std::env::var("XEN_PREVIEW_TOKEN")
                && token.len() >= 32
            {
                tauri::async_runtime::spawn(preview::serve(state.clone(), token));
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            current_workspace,
            connect_host,
            refresh_connection,
            host_details,
            host_performance,
            disconnect_host,
            remove_connection,
            start_vm_action,
            check_task,
            cancel_task
        ])
        .build(tauri::generate_context!())
        .expect("Build Xen Desktop")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(app.state::<Arc<DesktopState>>().workspace.close());
            }
        });
}
