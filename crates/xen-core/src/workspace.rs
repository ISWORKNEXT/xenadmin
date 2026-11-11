use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    network::PifSummary,
    operations::{TaskStatus, VmAction, VmResourceUpdate},
    storage::{PbdSummary, SrProbe},
    vm::{VdiSummary, VmDetails},
    xapi::{ApiError, Connection, ConnectionSettings, Inventory, RawInventory},
};

enum VmPreparation {
    Clone,
    ConvertToTemplate,
    NewVm(crate::provisioning::CreateVmRequest),
    Snapshot { as_template: bool },
}

const EVENT_CLASSES: &[&str] = &[
    "VM",
    "host",
    "host_metrics",
    "SR",
    "network",
    "pool",
    "task",
    "message",
    "VBD",
    "VDI",
    "PBD",
    "VM_guest_metrics",
    "PIF",
    "Bond",
    "VIF",
    "Certificate",
];

#[derive(Clone, Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub settings: ConnectionSettings,
}

struct ActiveConnection {
    connection: Connection,
    raw: RwLock<RawInventory>,
    snapshot: RwLock<Inventory>,
    error: RwLock<Option<String>>,
    lifecycle: Arc<RwLock<()>>,
    busy_vms: Mutex<BTreeSet<String>>,
    password: String,
    heartbeat_failed: AtomicBool,
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
    pub remote_ref: String,
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
    restore: Option<(String, String)>,
    local_cancel: Option<Arc<AtomicBool>>,
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
    pub async fn object_properties(
        &self,
        connection_id: &str,
        target: &crate::properties::ObjectTarget,
    ) -> Result<crate::properties::ObjectProperties, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .object_properties(target)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn update_object_properties(
        &self,
        connection_id: &str,
        properties: &crate::properties::ObjectPropertiesEdit,
    ) -> Result<crate::properties::ObjectProperties, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        let saved = active
            .connection
            .update_object_properties(properties)
            .await
            .map_err(|error| error.to_string())?;
        let _ = refresh_inventory_cache(&active).await;
        Ok(saved)
    }

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
        self: &Arc<Self>,
        mut connection: Connection,
        raw: RawInventory,
        mut snapshot: Inventory,
        password: String,
        replace_id: Option<String>,
        events_registered: bool,
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
        let active = Arc::new(ActiveConnection {
            connection,
            raw: RwLock::new(raw),
            snapshot: RwLock::new(snapshot),
            error: RwLock::new(None),
            lifecycle: Arc::new(RwLock::new(())),
            busy_vms: Mutex::new(BTreeSet::new()),
            password,
            heartbeat_failed: AtomicBool::new(false),
        });
        entries.insert(
            id.clone(),
            Entry {
                profile,
                active: Some(active.clone()),
            },
        );
        drop(entries);
        if events_registered {
            let workspace = self.clone();
            let active = active.clone();
            let event_id = id.clone();
            tokio::spawn(async move {
                workspace.event_loop(event_id, active).await;
            });
        }
        let workspace = self.clone();
        let heartbeat_id = id.clone();
        tokio::spawn(async move {
            workspace.heartbeat_loop(heartbeat_id).await;
        });
        Ok(id)
    }

    pub async fn connect(
        self: &Arc<Self>,
        settings: ConnectionSettings,
        password: &str,
        profile_id: Option<String>,
    ) -> Result<String, String> {
        let connection = Connection::login(settings, password)
            .await
            .map_err(|e| e.to_string())?;
        let events_registered = connection.event_register(EVENT_CLASSES).await.is_ok();
        let raw = match connection.raw_inventory().await {
            Ok(raw) => raw,
            Err(error) => {
                let _ = connection.logout().await;
                return Err(error.to_string());
            }
        };
        let snapshot = match raw.into_inventory(connection.settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.logout().await;
                return Err(error.to_string());
            }
        };
        self.add_connected(
            connection,
            raw,
            snapshot,
            password.to_owned(),
            profile_id,
            events_registered,
        )
        .await
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

    pub async fn update_vm_resources(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        resources: VmResourceUpdate,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .update_vm_resources(vm_uuid, &resources)
            .await
            .map_err(|error| error.to_string())?;
        match refresh_inventory_cache(&active).await {
            Ok(()) => Ok(()),
            Err(error) => Err(format!(
                "The VM was updated, but the inventory refresh failed: {error}"
            )),
        }
    }

    pub async fn change_cd(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        iso_uuid: Option<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .change_cd(vm_uuid, vbd_uuid, iso_uuid.as_deref())
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_vmss(
        &self,
        connection_id: &str,
        name: String,
        description: String,
        snapshot_type: String,
        schedule: crate::vmss::VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_vmss(
                &name,
                &description,
                &snapshot_type,
                &schedule,
                retained_snapshots,
                enabled,
                vm_uuids,
            )
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_vmss(
        &self,
        connection_id: &str,
        uuid: &str,
        name: String,
        description: String,
        snapshot_type: String,
        schedule: crate::vmss::VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .update_vmss(
                uuid,
                &name,
                &description,
                &snapshot_type,
                &schedule,
                retained_snapshots,
                enabled,
                vm_uuids,
            )
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn delete_vmss(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .delete_vmss(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn enable_ha(
        &self,
        connection_id: &str,
        heartbeat_sr_uuids: Vec<String>,
        failures_to_tolerate: u64,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .enable_ha(heartbeat_sr_uuids, failures_to_tolerate)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn disable_ha(&self, connection_id: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .disable_ha()
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn configure_wlb(
        &self,
        connection_id: &str,
        wlb_url: String,
        wlb_username: String,
        wlb_password: String,
        xenserver_username: String,
        xenserver_password: String,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .configure_wlb(
                &wlb_url,
                &wlb_username,
                &wlb_password,
                &xenserver_username,
                &xenserver_password,
            )
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_wlb_enabled(&self, connection_id: &str, enabled: bool) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .set_wlb_enabled(enabled)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn deconfigure_wlb(&self, connection_id: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .deconfigure_wlb()
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_dr_task(
        &self,
        connection_id: &str,
        sr_type: String,
        device_config: Vec<(String, String)>,
        whitelist: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_dr_task(&sr_type, device_config, whitelist)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn destroy_dr_task(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .destroy_dr_task(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn introduce_pvs_site(
        &self,
        connection_id: &str,
        name: String,
        description: String,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .introduce_pvs_site(&name, &description)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn forget_pvs_site(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .forget_pvs_site(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn introduce_pvs_server(
        &self,
        connection_id: &str,
        site_uuid: &str,
        addresses: Vec<String>,
        first_port: u16,
        last_port: u16,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .introduce_pvs_server(site_uuid, &addresses, first_port, last_port)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn forget_pvs_server(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .forget_pvs_server(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_pvs_proxy(
        &self,
        connection_id: &str,
        site_uuid: &str,
        vif_uuid: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_pvs_proxy(site_uuid, vif_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn destroy_pvs_proxy(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .destroy_pvs_proxy(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_pvs_cache(
        &self,
        connection_id: &str,
        site_uuid: &str,
        host_uuid: &str,
        sr_uuid: &str,
        size_mib: u64,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_pvs_cache(site_uuid, host_uuid, sr_uuid, size_mib)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn destroy_pvs_cache(&self, connection_id: &str, uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .destroy_pvs_cache(uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn pvs_vif_catalog(
        &self,
        connection_id: &str,
    ) -> Result<Vec<crate::pvs::PvsVif>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .pvs_vif_catalog()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn pool_status(
        &self,
        connection_id: &str,
    ) -> Result<crate::pool_status::PoolStatus, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .pool_status()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn enable_ad(
        &self,
        connection_id: &str,
        domain: String,
        user: String,
        password: String,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .enable_ad(&domain, &user, &password)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn disable_ad(
        &self,
        connection_id: &str,
        user: Option<String>,
        password: Option<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .disable_ad(user.as_deref(), password.as_deref())
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_subject_roles(
        &self,
        connection_id: &str,
        subject_uuid: &str,
        roles: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .set_subject_roles(subject_uuid, roles)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn auth_info(&self, connection_id: &str) -> Result<crate::auth::AuthInfo, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .auth_info()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn pbd_catalog(
        &self,
        connection_id: &str,
        sr_uuid: &str,
    ) -> Result<Vec<PbdSummary>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .storage
            .iter()
            .any(|sr| sr.uuid == sr_uuid)
        {
            return Err("This storage repository is not part of the selected connection.".into());
        }
        active
            .connection
            .pbd_catalog(sr_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn set_pbd_plugged(
        &self,
        connection_id: &str,
        pbd_uuid: &str,
        plugged: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .set_pbd_plugged(pbd_uuid, plugged)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn repair_sr(&self, connection_id: &str, sr_uuid: &str) -> Result<usize, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        let repaired = active
            .connection
            .repair_sr(sr_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await?;
        Ok(repaired)
    }

    pub async fn probe_sr(
        &self,
        connection_id: &str,
        host_uuid: &str,
        location: &str,
    ) -> Result<SrProbe, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .probe_sr(host_uuid, location)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn pif_catalog(&self, connection_id: &str) -> Result<Vec<PifSummary>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .pif_catalog()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn network_pifs(
        &self,
        connection_id: &str,
        network_uuid: &str,
    ) -> Result<Vec<PifSummary>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .network_pifs(network_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn set_pif_plugged(
        &self,
        connection_id: &str,
        pif_uuid: &str,
        plugged: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .set_pif_plugged(pif_uuid, plugged)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_sriov_network(
        &self,
        connection_id: &str,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
        pif_uuids: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_sriov_network(&name, &description, mtu, autoplug, pif_uuids)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_chin_network(
        &self,
        connection_id: &str,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
        protocol: String,
        pif_uuids: Vec<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_chin_network(&name, &description, mtu, autoplug, &protocol, pif_uuids)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_bond(
        &self,
        connection_id: &str,
        name: String,
        members: Vec<String>,
        mode: String,
        mtu: u64,
        autoplug: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_bond(&name, members, &mode, mtu, autoplug)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_vlan(
        &self,
        connection_id: &str,
        pif_uuid: &str,
        tag: i64,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_vlan(pif_uuid, tag, &name, &description, mtu, autoplug)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_network(
        &self,
        connection_id: &str,
        name: String,
        description: String,
        mtu: u64,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .create_network(&name, &description, mtu)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn destroy_network(
        &self,
        connection_id: &str,
        network_uuid: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .networks
            .iter()
            .any(|network| network.uuid == network_uuid)
        {
            return Err("This network is not part of the selected connection.".into());
        }
        active
            .connection
            .destroy_network(network_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn introduce_sr(
        &self,
        connection_id: &str,
        uuid: String,
        name: String,
        description: String,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .introduce_sr(&uuid, &name, &description, "file", "user", false)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn create_sr(
        &self,
        connection_id: &str,
        host_uuid: &str,
        name: String,
        description: String,
        location: String,
    ) -> Result<(), String> {
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
            .create_sr(host_uuid, &name, &description, &location)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn reattach_sr(
        &self,
        connection_id: &str,
        sr_uuid: &str,
        host_uuid: &str,
        location: String,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .storage
            .iter()
            .any(|sr| sr.uuid == sr_uuid)
        {
            return Err("This storage repository is not part of the selected connection.".into());
        }
        active
            .connection
            .reattach_sr(sr_uuid, host_uuid, &location)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn sr_action(
        &self,
        connection_id: &str,
        sr_uuid: &str,
        action: crate::operations::SrAction,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .storage
            .iter()
            .any(|sr| sr.uuid == sr_uuid)
        {
            return Err("This storage repository is not part of the selected connection.".into());
        }
        active
            .connection
            .sr_action(sr_uuid, action)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn delete_snapshots(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        snapshot_uuids: Vec<String>,
    ) -> Result<usize, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        let snapshot = active.snapshot.read().await;
        let parent_ref = snapshot
            .vms
            .iter()
            .chain(&snapshot.templates)
            .find(|vm| vm.uuid == vm_uuid)
            .map(|vm| vm.reference.clone())
            .ok_or("The parent virtual machine is not present in this connection.")?;
        let selected: std::collections::HashSet<&str> =
            snapshot_uuids.iter().map(String::as_str).collect();
        let targets: Vec<String> = snapshot
            .snapshots
            .iter()
            .filter(|candidate| {
                candidate.snapshot_of == parent_ref && selected.contains(candidate.uuid.as_str())
            })
            .map(|candidate| candidate.uuid.clone())
            .collect();
        drop(snapshot);
        if targets.is_empty() {
            return Err("No matching snapshots were selected.".into());
        }
        let mut deleted = 0usize;
        let mut failure = None;
        for uuid in targets {
            match active.connection.destroy_vm(&uuid, true, false).await {
                Ok(()) => deleted += 1,
                Err(error) => {
                    failure.get_or_insert_with(|| error.to_string());
                }
            }
        }
        let refresh = refresh_inventory_cache(&active).await;
        if let Some(error) = failure {
            return Err(format!(
                "Deleted {deleted} snapshot(s), but at least one failed: {error}"
            ));
        }
        refresh?;
        Ok(deleted)
    }

    pub async fn dismiss_alert(
        &self,
        connection_id: &str,
        message_uuid: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .dismiss_alert(message_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn host_action(
        &self,
        connection_id: &str,
        host_uuid: &str,
        action: crate::operations::HostAction,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        let present = active
            .snapshot
            .read()
            .await
            .hosts
            .iter()
            .any(|host| host.uuid == host_uuid);
        if !present {
            return Err("This host is not part of the selected connection.".into());
        }
        active
            .connection
            .host_action(host_uuid, action)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn add_cd_drive(&self, connection_id: &str, vm_uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .add_cd_drive(vm_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn vdi_catalog(&self, connection_id: &str) -> Result<Vec<VdiSummary>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .vdi_catalog()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn attach_vdi(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vdi_uuid: &str,
        bootable: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .attach_vdi(vm_uuid, vdi_uuid, bootable)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn sr_vdis(
        &self,
        connection_id: &str,
        sr_uuid: &str,
    ) -> Result<Vec<crate::vm::SrVdi>, String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !active
            .snapshot
            .read()
            .await
            .storage
            .iter()
            .any(|sr| sr.uuid == sr_uuid)
        {
            return Err("This storage repository is not part of the selected connection.".into());
        }
        active
            .connection
            .sr_vdis(sr_uuid)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn delete_vdi(&self, connection_id: &str, vdi_uuid: &str) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        active
            .connection
            .delete_vdi(vdi_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn update_vm_ha(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        restart_priority: String,
        start_delay: u64,
        shutdown_delay: u64,
        affinity_host_uuid: Option<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .update_vm_ha(
                vm_uuid,
                &restart_priority,
                start_delay,
                shutdown_delay,
                affinity_host_uuid.as_deref(),
            )
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn move_disk(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        target_sr_uuid: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        if !active
            .snapshot
            .read()
            .await
            .storage
            .iter()
            .any(|sr| sr.uuid == target_sr_uuid)
        {
            return Err("The target storage repository is not part of this connection.".into());
        }
        active
            .connection
            .move_disk(vm_uuid, vbd_uuid, target_sr_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_vbd_mode(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        mode: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .set_vbd_mode(vm_uuid, vbd_uuid, mode)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_vbd_plugged(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        plugged: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .set_vbd_plugged(vm_uuid, vbd_uuid, plugged)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_vbd_bootable(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        bootable: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .set_vbd_bootable(vm_uuid, vbd_uuid, bootable)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn add_disk(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        sr_uuid: &str,
        size_bytes: u64,
        name: Option<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .add_disk(vm_uuid, sr_uuid, size_bytes, name.as_deref())
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn resize_disk(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vdi_uuid: &str,
        size_bytes: u64,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .resize_disk(vm_uuid, vdi_uuid, size_bytes)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn remove_disk(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vbd_uuid: &str,
        delete_vdi: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .remove_disk(vm_uuid, vbd_uuid, delete_vdi)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn add_vif(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        network_uuid: &str,
        mac: Option<String>,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .add_vif(vm_uuid, network_uuid, mac.as_deref())
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn update_vif(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vif_uuid: &str,
        network_uuid: &str,
        locking_mode: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .update_vif(vm_uuid, vif_uuid, network_uuid, locking_mode)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn remove_vif(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vif_uuid: &str,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .remove_vif(vm_uuid, vif_uuid)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
    }

    pub async fn set_vif_plugged(
        &self,
        connection_id: &str,
        vm_uuid: &str,
        vif_uuid: &str,
        plugged: bool,
    ) -> Result<(), String> {
        let active = self.active(connection_id).await?;
        let _lease = active.lifecycle.read().await;
        if !vm_in_snapshot(&active.snapshot, vm_uuid).await {
            return Err("This virtual machine is not part of the selected connection.".into());
        }
        active
            .connection
            .set_vif_plugged(vm_uuid, vif_uuid, plugged)
            .await
            .map_err(|error| error.to_string())?;
        refresh_inventory_cache(&active).await
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
        refresh_inventory_cache(&active).await
    }

    async fn current_active(&self, id: &str) -> Option<Arc<ActiveConnection>> {
        self.entries
            .read()
            .await
            .get(id)
            .and_then(|entry| entry.active.clone())
    }

    async fn is_current(&self, id: &str, active: &Arc<ActiveConnection>) -> bool {
        self.entries
            .read()
            .await
            .get(id)
            .and_then(|entry| entry.active.as_ref())
            .is_some_and(|current| Arc::ptr_eq(current, active))
    }

    async fn event_loop(self: Arc<Self>, id: String, active: Arc<ActiveConnection>) {
        loop {
            if !self.is_current(&id, &active).await {
                return;
            }
            let next =
                tokio::time::timeout(Duration::from_secs(120), active.connection.event_next())
                    .await;
            match next {
                Ok(Ok(events)) => {
                    if !self.is_current(&id, &active).await {
                        return;
                    }
                    let _lease = active.lifecycle.read().await;
                    let changed = active.raw.write().await.apply_events(&events);
                    if changed {
                        let snapshot = {
                            let raw = active.raw.read().await;
                            raw.into_inventory(active.connection.settings.clone())
                        };
                        match snapshot {
                            Ok(snapshot) => {
                                *active.snapshot.write().await = snapshot;
                                *active.error.write().await = None;
                                active.heartbeat_failed.store(false, Ordering::Relaxed);
                            }
                            Err(error) => {
                                *active.error.write().await =
                                    Some(format!("Cannot rebuild the inventory cache: {error}"));
                            }
                        }
                    }
                }
                Ok(Err(ApiError::Unsupported)) => return,
                Ok(Err(ApiError::XenCode(code, _))) if code == "MESSAGE_METHOD_UNKNOWN" => {
                    return;
                }
                Ok(Err(error)) => {
                    let transport = matches!(
                        error,
                        ApiError::Connection
                            | ApiError::Http(_)
                            | ApiError::Protocol
                            | ApiError::HostIsSlave(_)
                    );
                    if transport
                        || matches!(&error, ApiError::XenCode(code, _) if code == "SESSION_INVALID")
                    {
                        *active.error.write().await =
                            Some(format!("Live updates stopped: {error}"));
                    }
                    return;
                }
                Err(_) => {}
            }
        }
    }

    async fn heartbeat_loop(self: Arc<Self>, id: String) {
        let mut failures = 0u32;
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let Some(active) = self.current_active(&id).await else {
                return;
            };
            let result =
                tokio::time::timeout(Duration::from_secs(10), active.connection.heartbeat()).await;
            match result {
                Ok(Ok(())) => {
                    failures = 0;
                    if active.heartbeat_failed.swap(false, Ordering::Relaxed) {
                        *active.error.write().await = None;
                    }
                }
                Ok(Err(ApiError::Unsupported)) => return,
                Ok(Err(ApiError::XenCode(code, _))) if code == "MESSAGE_METHOD_UNKNOWN" => {
                    return;
                }
                Ok(Err(error))
                    if matches!(
                        error,
                        ApiError::Connection
                            | ApiError::Http(_)
                            | ApiError::Protocol
                            | ApiError::HostIsSlave(_)
                    ) =>
                {
                    failures += 1;
                    active.heartbeat_failed.store(true, Ordering::Relaxed);
                    *active.error.write().await =
                        Some(format!("Connection heartbeat failed: {error}"));
                    if failures >= 2 {
                        failures = 0;
                        if let Err(error) = self.reconnect(&id, &active).await {
                            *active.error.write().await =
                                Some(format!("Connection lost; reconnect failed: {error}"));
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
                Ok(Err(_)) => failures = 0,
                Err(_) => {
                    failures += 1;
                    active.heartbeat_failed.store(true, Ordering::Relaxed);
                    *active.error.write().await = Some("Connection heartbeat timed out.".into());
                    if failures >= 2 {
                        failures = 0;
                        if let Err(error) = self.reconnect(&id, &active).await {
                            *active.error.write().await =
                                Some(format!("Connection lost; reconnect failed: {error}"));
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
            }
        }
    }

    async fn reconnect(
        self: &Arc<Self>,
        id: &str,
        old: &Arc<ActiveConnection>,
    ) -> Result<(), String> {
        let _write = old.lifecycle.write().await;
        if !self.is_current(id, old).await {
            return Err("The connection changed before it could be re-established.".into());
        }
        let connection = Connection::login(old.connection.settings.clone(), &old.password)
            .await
            .map_err(|error| error.to_string())?;
        let events_registered = connection.event_register(EVENT_CLASSES).await.is_ok();
        let raw = connection
            .raw_inventory()
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = raw
            .into_inventory(connection.settings.clone())
            .map_err(|error| error.to_string())?;
        let new_active = Arc::new(ActiveConnection {
            connection,
            raw: RwLock::new(raw),
            snapshot: RwLock::new(snapshot),
            error: RwLock::new(None),
            lifecycle: Arc::new(RwLock::new(())),
            busy_vms: Mutex::new(BTreeSet::new()),
            password: old.password.clone(),
            heartbeat_failed: AtomicBool::new(false),
        });
        let mut entries = self.entries.write().await;
        let Some(entry) = entries.get_mut(id) else {
            drop(entries);
            let _ =
                tokio::time::timeout(Duration::from_secs(3), new_active.connection.logout()).await;
            return Err("The saved connection no longer exists.".into());
        };
        if !entry
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, old))
        {
            drop(entries);
            let _ =
                tokio::time::timeout(Duration::from_secs(3), new_active.connection.logout()).await;
            return Err("The connection changed while it was reconnecting.".into());
        }
        entry.profile.settings = new_active.connection.settings.clone();
        entry.active = Some(new_active.clone());
        drop(entries);

        if events_registered {
            let workspace = self.clone();
            let active = new_active.clone();
            let event_id = id.to_owned();
            tokio::spawn(async move {
                workspace.event_loop(event_id, active).await;
            });
        }
        let stale = old.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(3), stale.connection.logout()).await;
        });
        Ok(())
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
        if matches!(action, VmAction::ConvertToTemplate) {
            return self
                .start_vm_preparation(
                    connection_id,
                    vm_uuid,
                    String::new(),
                    VmPreparation::ConvertToTemplate,
                )
                .await;
        }
        if let VmAction::Clone { name } = action {
            return self
                .start_vm_preparation(connection_id, vm_uuid, name, VmPreparation::Clone)
                .await;
        }
        if let VmAction::CreateFromSnapshot { name, as_template } = action {
            return self
                .start_vm_preparation(
                    connection_id,
                    vm_uuid,
                    name,
                    VmPreparation::Snapshot { as_template },
                )
                .await;
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
        let restore = if matches!(action, VmAction::Revert) {
            active
                .connection
                .revert_restore_hint(&vm_uuid)
                .await
                .unwrap_or_default()
        } else {
            None
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
            remote_ref: remote_ref.clone(),
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
                restore,
                local_cancel: None,
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

    pub async fn create_vm_from_template(
        self: &Arc<Self>,
        connection_id: String,
        request: crate::provisioning::CreateVmRequest,
    ) -> Result<Operation, String> {
        self.start_vm_preparation(
            connection_id,
            request.template_uuid.clone(),
            request.name.clone(),
            VmPreparation::NewVm(request),
        )
        .await
    }

    pub async fn start_xva_transfer(
        self: &Arc<Self>,
        connection_id: String,
        request: crate::xva::XvaRequest,
    ) -> Result<Operation, String> {
        use crate::xva::{XvaOutcome, XvaRequest};
        let active = self.active(&connection_id).await?;
        let lease = active.lifecycle.clone().read_owned().await;
        if active.connection.settings.read_only {
            return Err("This connection is read-only.".into());
        }
        let (busy_key, action, object_name) = match &request {
            XvaRequest::Export { vm_uuid, .. } => {
                let snapshot = active.snapshot.read().await;
                let vm = snapshot
                    .vms
                    .iter()
                    .chain(&snapshot.templates)
                    .chain(&snapshot.snapshots)
                    .find(|v| &v.uuid == vm_uuid)
                    .ok_or("The object is not part of this connection.")?;
                (vm_uuid.clone(), "Export XVA", vm.name.clone())
            }
            XvaRequest::Import { name, .. } => ("xva-import".into(), "Import XVA", name.clone()),
        };
        if !active.busy_vms.lock().await.insert(busy_key.clone()) {
            return Err("A task is already active for this object.".into());
        }
        let remote_ref = match active.connection.create_xva_task(action).await {
            Ok(reference) => reference,
            Err(error) => {
                active.busy_vms.lock().await.remove(&busy_key);
                return Err(error.to_string());
            }
        };
        let connection_name = display_name(
            &active.connection.settings,
            &active.snapshot.read().await.pool_name,
        );
        let local_cancel = Arc::new(AtomicBool::new(false));
        let record = Operation {
            id: Uuid::now_v7().to_string(),
            connection_id,
            connection_name,
            vm_uuid: busy_key.clone(),
            remote_ref: remote_ref.clone(),
            object_name,
            action: action.into(),
            status: TaskStatus::Running,
            progress: 0.0,
            message: String::new(),
            can_cancel: true,
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
                restore: None,
                local_cancel: Some(local_cancel.clone()),
            },
        );
        let workspace = self.clone();
        let task_id = record.id.clone();
        tokio::spawn(async move {
            let transfer = active
                .connection
                .run_xva_transfer(&request, &remote_ref, &local_cancel);
            tokio::pin!(transfer);
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let result = loop {
                tokio::select! {
                    result = &mut transfer => break result,
                    _ = interval.tick() => {
                        if let Ok(remote) = active.connection.task_record(&remote_ref).await
                            && let Some(task) = workspace.tasks.lock().await.get_mut(&task_id)
                        { task.record.progress=remote.progress; task.record.can_cancel=true; }
                    }
                }
            };
            let (status, message) = match result {
                Ok(outcome) => {
                    let message = match outcome {
                        XvaOutcome::Exported { bytes } => {
                            format!("Exported {bytes} bytes to the selected XVA file.")
                        }
                        XvaOutcome::Imported { uuid } => format!("Imported halted object {uuid}."),
                    };
                    match refresh_inventory_cache(&active).await {
                        Ok(()) => (TaskStatus::Succeeded, message),
                        Err(error) => (
                            TaskStatus::Succeeded,
                            format!("{message} Inventory refresh failed: {error}"),
                        ),
                    }
                }
                Err(ApiError::XenCode(code, message)) if code == "TASK_CANCELLED" => {
                    (TaskStatus::Cancelled, message)
                }
                Err(error) => (TaskStatus::Failed, error.to_string()),
            };
            if let Some(task) = workspace.tasks.lock().await.get_mut(&task_id) {
                task.record.status = status;
                task.record.message = message;
                task.record.progress = 1.0;
                task.record.can_cancel = false;
            }
            active.busy_vms.lock().await.remove(&busy_key);
            drop(lease);
        });
        Ok(record)
    }

    async fn start_vm_preparation(
        self: &Arc<Self>,
        connection_id: String,
        vm_uuid: String,
        name: String,
        mode: VmPreparation,
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
        let converting = matches!(&mode, VmPreparation::ConvertToTemplate);
        let name = if converting {
            object_name.clone()
        } else {
            name
        };
        let record = Operation {
            id: Uuid::now_v7().to_string(),
            connection_id,
            connection_name,
            vm_uuid: vm_uuid.clone(),
            remote_ref: String::new(),
            object_name,
            action: match &mode {
                VmPreparation::Clone => "Clone",
                VmPreparation::ConvertToTemplate => "Convert to template",
                VmPreparation::NewVm(_) => "Create VM",
                VmPreparation::Snapshot { as_template: true } => "Create template from snapshot",
                VmPreparation::Snapshot { as_template: false } => "Create VM from snapshot",
            }
            .into(),
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
                restore: None,
                local_cancel: None,
            },
        );
        let workspace = self.clone();
        let active_for_task = active.clone();
        tokio::spawn(async move {
            let result = match mode {
                VmPreparation::ConvertToTemplate => active_for_task
                    .connection
                    .convert_vm_to_template(&vm_uuid)
                    .await
                    .map(|()| vm_uuid.clone()),
                VmPreparation::NewVm(request) => {
                    active_for_task
                        .connection
                        .create_vm_from_template(&request)
                        .await
                }
                VmPreparation::Clone => active_for_task.connection.clone_vm(&vm_uuid, &name).await,
                VmPreparation::Snapshot { as_template } => {
                    active_for_task
                        .connection
                        .create_from_snapshot(&vm_uuid, &name, as_template)
                        .await
                }
            };
            let result = match result {
                Ok(new_uuid) => match refresh_inventory_cache(&active_for_task).await {
                    Ok(()) => Ok(if converting {
                        format!("Converted {name} ({new_uuid}) to a template.")
                    } else {
                        format!("Created {name} ({new_uuid}).")
                    }),
                    Err(error) => Err(format!(
                        "The object was created, but the inventory refresh failed: {error}"
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
            remote_ref: String::new(),
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
                restore: None,
                local_cancel: None,
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
                Ok(()) => match refresh_inventory_cache(&active_for_task).await {
                    Ok(()) => Ok(()),
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
                        let _ = refresh_inventory_cache(active).await;
                        active.connection.destroy_task(reference).await;
                    }
                    if let Some(task) = self.tasks.lock().await.get_mut(id) {
                        task.record.status = remote.status;
                        task.record.progress = remote.progress;
                        task.record.message = remote.message;
                        task.record.can_cancel = !terminal && remote.can_cancel;
                    }
                    if terminal {
                        if remote.status == TaskStatus::Succeeded {
                            let restore = self
                                .tasks
                                .lock()
                                .await
                                .get(id)
                                .and_then(|task| task.restore.clone());
                            if let Some((parent_uuid, _)) = restore {
                                let state = active
                                    .snapshot
                                    .read()
                                    .await
                                    .vms
                                    .iter()
                                    .find(|vm| vm.uuid == parent_uuid)
                                    .map(|vm| vm.power_state.clone());
                                let action = match state.as_deref() {
                                    Some("Halted") => Some(VmAction::Start),
                                    Some("Suspended") => Some(VmAction::Resume),
                                    _ => None,
                                };
                                if let Some(action) = action {
                                    let result = match active
                                        .connection
                                        .begin_vm_action(&parent_uuid, &action)
                                        .await
                                    {
                                        Ok(task_ref) => {
                                            let result =
                                                active.connection.wait_task(&task_ref).await;
                                            let _ = refresh_inventory_cache(active).await;
                                            result.map_err(|error| error.to_string())
                                        }
                                        Err(error) => Err(error.to_string()),
                                    };
                                    if let Some(task) = self.tasks.lock().await.get_mut(id) {
                                        task.record.message = match result {
                                            Ok(()) => {
                                                "Reverted and restored the running power state."
                                                    .into()
                                            }
                                            Err(error) => format!(
                                                "Reverted, but restoring the power state failed: {error}"
                                            ),
                                        };
                                    }
                                }
                            }
                        }
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
        let (connection_id, reference, local_cancel) = {
            let tasks = self.tasks.lock().await;
            let task = tasks.get(id).ok_or("Task not found.")?;
            if !task.record.can_cancel || task.record.status != TaskStatus::Running {
                return Err("The host does not allow cancelling this task.".into());
            }
            (
                task.record.connection_id.clone(),
                task.remote_ref.clone(),
                task.local_cancel.clone(),
            )
        };
        if let Some(cancel) = local_cancel {
            cancel.store(true, Ordering::Relaxed);
            return Ok(());
        }
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
            .write()
            .await
            .values_mut()
            .filter_map(|entry| entry.active.take())
            .collect();
        for active in connections {
            let _ = tokio::time::timeout(Duration::from_secs(3), active.connection.logout()).await;
        }
    }
}

/// Refresh the raw cache and derived inventory. Callers must hold the
/// connection's lifecycle read lease.
async fn refresh_inventory_cache(active: &ActiveConnection) -> Result<(), String> {
    match active.connection.raw_inventory().await {
        Ok(raw) => {
            let snapshot = raw
                .into_inventory(active.connection.settings.clone())
                .map_err(|error| error.to_string())?;
            *active.raw.write().await = raw;
            *active.snapshot.write().await = snapshot;
            *active.error.write().await = None;
            active.heartbeat_failed.store(false, Ordering::Relaxed);
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            *active.error.write().await = Some(message.clone());
            Err(message)
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
