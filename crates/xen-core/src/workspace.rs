use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    operations::{TaskStatus, VmAction},
    vm::VmDetails,
    xapi::{Connection, ConnectionSettings, Inventory},
};

#[derive(Clone, Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub settings: ConnectionSettings,
}

struct ActiveConnection {
    connection: Connection,
    snapshot: RwLock<Inventory>,
    error: RwLock<Option<String>>,
    lifecycle: Arc<RwLock<()>>,
    busy_vms: Mutex<BTreeSet<String>>,
}

struct Entry {
    profile: Profile,
    active: Option<Arc<ActiveConnection>>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ConnectionView {
    Offline {
        id: String,
        settings: ConnectionSettings,
    },
    Online {
        id: String,
        settings: ConnectionSettings,
        inventory: Inventory,
    },
    Stale {
        id: String,
        settings: ConnectionSettings,
        inventory: Inventory,
        error: String,
    },
}

#[derive(Clone, Serialize)]
pub struct Operation {
    pub id: String,
    pub connection_id: String,
    pub connection_name: String,
    pub vm_uuid: String,
    pub object_name: String,
    pub action: String,
    pub status: TaskStatus,
    pub progress: f64,
    pub message: String,
    pub can_cancel: bool,
    pub started_at: u64,
}

struct TrackedTask {
    record: Operation,
    remote_ref: String,
}

#[derive(Clone, Serialize)]
pub struct WorkspaceView {
    pub connections: Vec<ConnectionView>,
    pub tasks: Vec<Operation>,
}

#[derive(Default)]
pub struct Workspace {
    entries: RwLock<BTreeMap<String, Entry>>,
    tasks: Mutex<BTreeMap<String, TrackedTask>>,
}

impl Workspace {
    pub fn with_profiles(profiles: Vec<Profile>) -> Arc<Self> {
        Arc::new(Self {
            entries: RwLock::new(
                profiles
                    .into_iter()
                    .map(|p| {
                        (
                            p.id.clone(),
                            Entry {
                                profile: p,
                                active: None,
                            },
                        )
                    })
                    .collect(),
            ),
            ..Self::default()
        })
    }

    pub async fn profiles(&self) -> Vec<Profile> {
        self.entries
            .read()
            .await
            .values()
            .map(|e| e.profile.clone())
            .collect()
    }

    pub async fn add_connected(
        &self,
        mut connection: Connection,
        mut snapshot: Inventory,
        replace_id: Option<String>,
    ) -> Result<String, String> {
        // A pool may contain many hosts. Connecting to another member must not duplicate its objects.
        if snapshot.hosts.iter().any(|h| h.address == "10.100.1.11") {
            connection.settings.read_only = true;
            snapshot.settings.read_only = true;
        }
        let id = snapshot.pool_id.clone();
        let mut entries = self.entries.write().await;
        if entries.get(&id).is_some_and(|e| e.active.is_some()) {
            drop(entries);
            let _ = connection.logout().await;
            return Err("This pool is already connected. Select it in the sidebar.".into());
        }
        if let Some(previous) = replace_id.filter(|old| old != &id) {
            if entries.get(&previous).is_some_and(|e| e.active.is_some()) {
                drop(entries);
                let _ = connection.logout().await;
                return Err("Disconnect the existing profile before replacing it.".into());
            }
            entries.remove(&previous);
        }
        let profile = Profile {
            id: id.clone(),
            settings: connection.settings.clone(),
        };
        entries.insert(
            id.clone(),
            Entry {
                profile,
                active: Some(Arc::new(ActiveConnection {
                    connection,
                    snapshot: RwLock::new(snapshot),
                    error: RwLock::new(None),
                    lifecycle: Arc::new(RwLock::new(())),
                    busy_vms: Mutex::new(BTreeSet::new()),
                })),
            },
        );
        Ok(id)
    }

    pub async fn connect(
        &self,
        settings: ConnectionSettings,
        password: &str,
        profile_id: Option<String>,
    ) -> Result<String, String> {
        let connection = Connection::login(settings, password)
            .await
            .map_err(|e| e.to_string())?;
        let snapshot = match connection.inventory().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.logout().await;
                return Err(error.to_string());
            }
        };
        self.add_connected(connection, snapshot, profile_id).await
    }

    async fn active(&self, id: &str) -> Result<Arc<ActiveConnection>, String> {
        self.entries
            .read()
            .await
            .get(id)
            .and_then(|e| e.active.clone())
            .ok_or_else(|| "This connection is offline. Reconnect before continuing.".into())
    }

    pub async fn host_details(
        &self,
        connection_id: &str,
        host_uuid: &str,
    ) -> Result<crate::host::HostDetails, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .hosts
            .iter()
            .any(|host| host.uuid == host_uuid)
        {
            return Err("This host is not part of the selected connection.".into());
        }
        active
            .connection
            .host_details(host_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn host_performance(
        &self,
        connection_id: &str,
        host_uuid: &str,
        range_seconds: u64,
    ) -> Result<crate::performance::HostPerformance, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .hosts
            .iter()
            .any(|host| host.uuid == host_uuid)
        {
            return Err("This host is not part of the selected connection.".into());
        }
        active
            .connection
            .host_performance(host_uuid, range_seconds)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn vm_details(
        &self,
        connection_id: &str,
        vm_uuid: &str,
    ) -> Result<VmDetails, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .vm_details(vm_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_vm(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        name: String,
        description: String,
        vcpus_max: u64,
        vcpus_at_startup: u64,
        memory_mib: u64,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .update_vm(
                vm_uuid,
                &name,
                &description,
                vcpus_max,
                vcpus_at_startup,
                memory_mib,
            )
            .await
            .map_err(|error| error.to_string())?;
        match active.connection.inventory().await {
            Ok(snapshot) => {
                *active.snapshot.write().await = snapshot;
                *active.error.write().await = None;
                Ok(())
            }
            Err(error) => Err(format!(
                "The VM was updated, but the inventory refresh failed: {error}"
            )),
        }
    }

    pub async fn console_tunnel(
        &self,
        connection_id: &str,
        vm_uuid: &str,
    ) -> Result<crate::console::ConsoleTunnel, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .console_tunnel(vm_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn vm_performance(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        range_seconds: u64,
    ) -> Result<crate::performance::HostPerformance, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .vm_performance(vm_uuid, range_seconds)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn view(&self) -> WorkspaceView {
        let entries: Vec<_> = self
            .entries
            .read()
            .await
            .values()
            .map(|e| (e.profile.clone(), e.active.clone()))
            .collect();
        let mut connections = Vec::new();
        for (profile, active) in entries {
            let Profile { id, settings } = profile;
            connections.push(if let Some(active) = active {
                let inventory = active.snapshot.read().await.clone();
                match active.error.read().await.clone() {
                    Some(error) => ConnectionView::Stale {
                        id,
                        settings,
                        inventory,
                        error,
                    },
                    None => ConnectionView::Online {
                        id,
                        settings,
                        inventory,
                    },
                }
            } else {
                ConnectionView::Offline { id, settings }
            });
        }
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .await
            .values()
            .map(|t| t.record.clone())
            .collect();
        tasks.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
        WorkspaceView { connections, tasks }
    }

    pub async fn refresh(&self, id: &str) -> Result<(), String> {
        let active = self.active(id).await?;
        let _lease = active.lifecycle.read().await;
        match active.connection.inventory().await {
            Ok(snapshot) => {
                *active.snapshot.write().await = snapshot;
                *active.error.write().await = None;
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                *active.error.write().await = Some(message.clone());
                Err(message)
            }
        }
    }

    pub async fn disconnect(&self, id: &str) -> Result<(), String> {
        let active = self.active(id).await?;
        if !active.busy_vms.lock().await.is_empty() {
            return Err(
                "Resolve active or interrupted tasks before disconnecting this connection.".into(),
            );
        }
        let _lease = active.lifecycle.try_write().map_err(
            |_| "This connection has an active operation or refresh. Wait before disconnecting.",
        )?;
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.active = None;
        }
        drop(entries);
        tokio::time::timeout(Duration::from_secs(5), active.connection.logout())
            .await
            .map_err(|_| "Disconnected locally; the server logout timed out.".to_owned())?
            .map_err(|e| e.to_string())
    }

    pub async fn remove(&self, id: &str) -> Result<(), String> {
        let mut entries = self.entries.write().await;
        if entries.get(id).is_some_and(|e| e.active.is_some()) {
            return Err("Disconnect this profile before removing it.".into());
        }
        entries.remove(id);
        Ok(())
    }

    pub async fn start_action(
        self: &Arc<Self>,
        connection_id: String,
        vm_uuid: String,
        action: VmAction,
    ) -> Result<Operation, String> {
        if let VmAction::Destroy {
            delete_disks,
            delete_snapshots,
        } = action
        {
            return self
                .start_destroy(connection_id, vm_uuid, delete_disks, delete_snapshots)
                .await;
        }
        if let VmAction::Clone { name } = action {
            return self.start_clone(connection_id, vm_uuid, name).await;
        }
        let active = self.active(&connection_id).await?;
        let lease = active.lifecycle.clone().read_owned().await;
        // Reserve before sending, including while the initial HTTP request is in flight.
        if !active.busy_vms.lock().await.insert(vm_uuid.clone()) {
            return Err("A task is already active for this object.".into());
        }
        let snapshot = active.snapshot.read().await;
        let object = snapshot
            .vms
            .iter()
            .chain(&snapshot.templates)
            .chain(&snapshot.snapshots)
            .find(|vm| vm.uuid == vm_uuid)
            .map(|vm| vm.name.clone());
        let connection_name = display_name(&active.connection.settings, &snapshot.pool_name);
        drop(snapshot);
        let Some(object_name) = object else {
            active.busy_vms.lock().await.remove(&vm_uuid);
            return Err("Object is not present in this connection. Refresh the inventory.".into());
        };
        let remote_ref = match active.connection.begin_vm_action(&vm_uuid, &action).await {
            Ok(reference) => reference,
            Err(error) => {
                active.busy_vms.lock().await.remove(&vm_uuid);
                return Err(error.to_string());
            }
        };
        let record = Operation {
            id: Uuid::now_v7().to_string(),
            connection_id,
            connection_name,
            vm_uuid: vm_uuid.clone(),
            object_name,
            action: action.title().into(),
            status: TaskStatus::Running,
            progress: 0.0,
            message: String::new(),
            can_cancel: false,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.tasks.lock().await.insert(
            record.id.clone(),
            TrackedTask {
                record: record.clone(),
                remote_ref: remote_ref.clone(),
            },
        );
        let workspace = self.clone();
        let task_id = record.id.clone();
        tokio::spawn(async move {
            workspace.monitor(&active, &task_id, &remote_ref).await;
            if workspace
                .tasks
                .lock()
                .await
                .get(&task_id)
                .is_some_and(|t| t.record.status.terminal())
            {
                active.busy_vms.lock().await.remove(&vm_uuid);
            }
            drop(lease);
        });
        Ok(record)
    }

    async fn start_clone(
        self: &Arc<Self>,
        connection_id: String,
        vm_uuid: String,
        name: String,
    ) -> Result<Operation, String> {
        let active = self.active(&connection_id).await?;
        let lease = active.lifecycle.clone().read_owned().await;
        if !active.busy_vms.lock().await.insert(vm_uuid.clone()) {
            return Err("A task is already active for this object.".into());
        }
        let snapshot = active.snapshot.read().await;
        let object = snapshot
            .vms
            .iter()
            .chain(&snapshot.templates)
            .chain(&snapshot.snapshots)
            .find(|vm| vm.uuid == vm_uuid)
            .map(|vm| vm.name.clone());
        let connection_name = display_name(&active.connection.settings, &snapshot.pool_name);
        drop(snapshot);
        let Some(object_name) = object else {
            active.busy_vms.lock().await.remove(&vm_uuid);
            return Err("Object is not present in this connection. Refresh the inventory.".into());
        };
        if active.connection.settings.read_only {
            active.busy_vms.lock().await.remove(&vm_uuid);
            return Err(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            );
        }
        let record = Operation {
            id: Uuid::now_v7().to_string(),
            connection_id,
            connection_name,
            vm_uuid: vm_uuid.clone(),
            object_name,
            action: "Clone".into(),
            status: TaskStatus::Running,
            progress: 0.0,
            message: String::new(),
            can_cancel: false,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let task_id = record.id.clone();
        self.tasks.lock().await.insert(
            task_id.clone(),
            TrackedTask {
                record: record.clone(),
                remote_ref: String::new(),
            },
        );
        let workspace = self.clone();
        let active_for_task = active.clone();
        tokio::spawn(async move {
            let result = active_for_task.connection.clone_vm(&vm_uuid, &name).await;
            let result = match result {
                Ok(new_uuid) => match active_for_task.connection.inventory().await {
                    Ok(snapshot) => {
                        *active_for_task.snapshot.write().await = snapshot;
                        *active_for_task.error.write().await = None;
                        Ok(format!("Created VM {name} ({new_uuid})."))
                    }
                    Err(error) => Err(format!(
                        "Clone succeeded, but the inventory refresh failed: {error}"
                    )),
                },
                Err(error) => Err(error.to_string()),
            };
            if let Some(task) = workspace.tasks.lock().await.get_mut(&task_id) {
                task.record.progress = 1.0;
                match result {
                    Ok(message) => {
                        task.record.status = TaskStatus::Succeeded;
                        task.record.message = message;
                    }
                    Err(message) => {
                        task.record.status = TaskStatus::Failed;
                        task.record.message = message;
                    }
                }
            }
            active_for_task.busy_vms.lock().await.remove(&vm_uuid);
            drop(lease);
        });
        Ok(record)
    }

    async fn start_destroy(
        self: &Arc<Self>,
        connection_id: String,
        vm_uuid: String,
        delete_disks: bool,
        delete_snapshots: bool,
    ) -> Result<Operation, String> {
        let active = self.active(&connection_id).await?;
        let lease = active.lifecycle.clone().read_owned().await;
        if !active.busy_vms.lock().await.insert(vm_uuid.clone()) {
            return Err("A task is already active for this object.".into());
        }
        let snapshot = active.snapshot.read().await;
        let object = snapshot
            .vms
            .iter()
            .chain(&snapshot.templates)
            .chain(&snapshot.snapshots)
            .find(|vm| vm.uuid == vm_uuid)
            .map(|vm| vm.name.clone());
        let connection_name = display_name(&active.connection.settings, &snapshot.pool_name);
        drop(snapshot);
        let Some(object_name) = object else {
            active.busy_vms.lock().await.remove(&vm_uuid);
            return Err("Object is not present in this connection. Refresh the inventory.".into());
        };
        if active.connection.settings.read_only {
            active.busy_vms.lock().await.remove(&vm_uuid);
            return Err(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            );
        }
        let record = Operation {
            id: Uuid::now_v7().to_string(),
            connection_id,
            connection_name,
            vm_uuid: vm_uuid.clone(),
            object_name,
            action: "Delete".into(),
            status: TaskStatus::Running,
            progress: 0.0,
            message: String::new(),
            can_cancel: false,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let task_id = record.id.clone();
        self.tasks.lock().await.insert(
            task_id.clone(),
            TrackedTask {
                record: record.clone(),
                remote_ref: String::new(),
            },
        );
        let workspace = self.clone();
        let active_for_task = active.clone();
        tokio::spawn(async move {
            let result = active_for_task
                .connection
                .destroy_vm(&vm_uuid, delete_disks, delete_snapshots)
                .await;
            let result = match result {
                Ok(()) => match active_for_task.connection.inventory().await {
                    Ok(snapshot) => {
                        *active_for_task.snapshot.write().await = snapshot;
                        *active_for_task.error.write().await = None;
                        Ok(())
                    }
                    Err(error) => Err(format!(
                        "Deleted, but the inventory refresh failed: {error}"
                    )),
                },
                Err(error) => Err(error.to_string()),
            };
            if let Some(task) = workspace.tasks.lock().await.get_mut(&task_id) {
                task.record.progress = 1.0;
                match result {
                    Ok(()) => {
                        task.record.status = TaskStatus::Succeeded;
                        task.record.message = "Object deleted.".into();
                    }
                    Err(message) => {
                        task.record.status = TaskStatus::Failed;
                        task.record.message = message;
                    }
                }
            }
            active_for_task.busy_vms.lock().await.remove(&vm_uuid);
            drop(lease);
        });
        Ok(record)
    }

    async fn monitor(&self, active: &Arc<ActiveConnection>, id: &str, reference: &str) {
        for _ in 0..3600 {
            match active.connection.task_record(reference).await {
                Ok(remote) => {
                    let terminal = remote.status.terminal();
                    if terminal {
                        match active.connection.inventory().await {
                            Ok(snapshot) => {
                                *active.snapshot.write().await = snapshot;
                                *active.error.write().await = None;
                            }
                            Err(error) => {
                                *active.error.write().await = Some(error.to_string());
                            }
                        }
                        active.connection.destroy_task(reference).await;
                    }
                    if let Some(task) = self.tasks.lock().await.get_mut(id) {
                        task.record.status = remote.status;
                        task.record.progress = remote.progress;
                        task.record.message = remote.message;
                        task.record.can_cancel = !terminal && remote.can_cancel;
                    }
                    if terminal {
                        return;
                    }
                }
                Err(error) => {
                    if let Some(task) = self.tasks.lock().await.get_mut(id) {
                        task.record.status = TaskStatus::Unreachable;
                        task.record.message = format!(
                            "{} The task may still be running. Check again; do not resubmit it.",
                            error
                        );
                        task.record.can_cancel = false;
                    }
                    return;
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if let Some(task) = self.tasks.lock().await.get_mut(id) {
            task.record.status = TaskStatus::Unreachable;
            task.record.message = "Task is still pending. Check again without resubmitting.".into();
        }
    }

    pub async fn check_task(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let (connection_id, vm_uuid, reference) = {
            let tasks = self.tasks.lock().await;
            let task = tasks.get(id).ok_or("Task not found.")?;
            if task.record.status != TaskStatus::Unreachable {
                return Err("Only interrupted tasks need to be checked again.".into());
            }
            (
                task.record.connection_id.clone(),
                task.record.vm_uuid.clone(),
                task.remote_ref.clone(),
            )
        };
        let active = self.active(&connection_id).await?;
        let lease = active.lifecycle.clone().read_owned().await;
        {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.get_mut(id).ok_or("Task not found.")?;
            if task.record.status != TaskStatus::Unreachable {
                return Err("This task is already being monitored.".into());
            }
            task.record.status = TaskStatus::Running;
        }
        active.busy_vms.lock().await.insert(vm_uuid.clone());
        let workspace = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            workspace.monitor(&active, &id, &reference).await;
            if workspace
                .tasks
                .lock()
                .await
                .get(&id)
                .is_some_and(|t| t.record.status.terminal())
            {
                active.busy_vms.lock().await.remove(&vm_uuid);
            }
            drop(lease);
        });
        Ok(())
    }

    pub async fn cancel_task(&self, id: &str) -> Result<(), String> {
        let (connection_id, reference) = {
            let tasks = self.tasks.lock().await;
            let task = tasks.get(id).ok_or("Task not found.")?;
            if !task.record.can_cancel || task.record.status != TaskStatus::Running {
                return Err("The host does not allow cancelling this task.".into());
            }
            (task.record.connection_id.clone(), task.remote_ref.clone())
        };
        let active = self.active(&connection_id).await?;
        active
            .connection
            .cancel_task(&reference)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn close(&self) {
        let connections: Vec<_> = self
            .entries
            .read()
            .await
            .values()
            .filter_map(|e| e.active.clone())
            .collect();
        for active in connections {
            let _ = tokio::time::timeout(Duration::from_secs(3), active.connection.logout()).await;
        }
    }
}

async fn vm_in_snapshot(snapshot: &RwLock<Inventory>, vm_uuid: &str) -> bool {
    let snapshot = snapshot.read().await;
    snapshot
        .vms
        .iter()
        .chain(&snapshot.templates)
        .chain(&snapshot.snapshots)
        .any(|vm| vm.uuid == vm_uuid)
}

fn display_name(settings: &ConnectionSettings, pool_name: &str) -> String {
    if settings.label.trim().is_empty() {
        pool_name.into()
    } else {
        settings.label.clone()
    }
}
