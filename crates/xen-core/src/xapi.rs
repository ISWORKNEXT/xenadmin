use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use reqwest::Url;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::transport::{HostTransport, TlsOptions};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Enter a valid HTTPS host address without credentials or a path.")]
    InvalidHost,
    #[error("Connection failed. Check the host address and TLS certificate setting.")]
    Connection,
    #[error("The server returned HTTP {0}.")]
    Http(u16),
    #[error("The server returned an invalid XenAPI response.")]
    Protocol,
    #[error("This XenAPI method is not supported by this server.")]
    Unsupported,
    #[error("{0}")]
    Xen(String),
    #[error("{}", .0.message)]
    CopyInterrupted(Box<crate::provisioning::CopyRecovery>),
    #[error("{1}")]
    XenCode(String, String),
    #[error("This host is a pool member. Reconnect to the coordinator at {0}.")]
    HostIsSlave(String),
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ConnectionSettings {
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub tls: TlsOptions,
    /// Per-connection HTTP/SOCKS proxy, e.g. `http://proxy:3128`.
    #[serde(default)]
    pub proxy: Option<String>,
}

impl ConnectionSettings {
    fn enforce_host_policy(&mut self, origin: &Url) {
        if origin.host_str() == Some("10.100.1.11") {
            self.read_only = true;
        }
    }

    /// Validate the optional proxy URL before any network request.
    pub fn proxy_url(&self) -> Result<Option<Url>, String> {
        let Some(proxy) = self
            .proxy
            .as_deref()
            .map(str::trim)
            .filter(|proxy| !proxy.is_empty())
        else {
            return Ok(None);
        };
        let url = Url::parse(proxy).map_err(|_| {
            "Enter a proxy URL like http://proxy:3128 or socks5://proxy:1080.".to_owned()
        })?;
        if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
            return Err("Proxy scheme must be http, https, socks5 or socks5h.".to_owned());
        }
        if url.host_str().is_none() {
            return Err("The proxy URL must include a host.".to_owned());
        }
        if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
            return Err("The proxy URL must not include a path, query or fragment.".to_owned());
        }
        Ok(Some(url))
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    id: u64,
    #[serde(default)]
    result: Value,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    message: String,
    #[serde(default)]
    data: Vec<Value>,
}

pub struct Connection {
    pub(crate) transport: HostTransport,
    pub(crate) session: String,
    next_id: AtomicU64,
    pub settings: ConnectionSettings,
}

impl Connection {
    pub async fn login(mut settings: ConnectionSettings, password: &str) -> Result<Self, ApiError> {
        for attempt in 0..2 {
            let address = if settings.host.contains("://") {
                settings.host.clone()
            } else {
                format!("https://{}", settings.host)
            };
            let origin = Url::parse(&address).map_err(|_| ApiError::InvalidHost)?;
            settings.enforce_host_policy(&origin);
            let transport = HostTransport::new(origin, settings.tls, settings.proxy.as_deref())
                .map_err(|error| match error {
                    crate::transport::TransportError::InvalidProxy => {
                        ApiError::Xen("The proxy URL is invalid.".into())
                    }
                    _ => ApiError::InvalidHost,
                })?;
            let mut connection = Self {
                transport,
                session: String::new(),
                next_id: AtomicU64::new(1),
                settings: settings.clone(),
            };
            match connection
                .request(
                    "session.login_with_password",
                    json!([
                        connection.settings.username,
                        password,
                        "1.0",
                        "Xen Desktop/0.1"
                    ]),
                )
                .await
            {
                Ok(session) => {
                    connection.session = session;
                    return Ok(connection);
                }
                Err(ApiError::HostIsSlave(master)) if attempt == 0 && !master.is_empty() => {
                    settings.host = master;
                }
                Err(error) => return Err(error),
            }
        }
        Err(ApiError::InvalidHost)
    }

    pub(crate) async fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, ApiError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let response = self
            .transport
            .post_json_rpc(&json!({
                "jsonrpc": "2.0", "id": id, "method": method, "params": params
            }))
            .await
            .map_err(|_| ApiError::Connection)?;
        if !response.status().is_success() {
            return Err(ApiError::Http(response.status().as_u16()));
        }
        let response: RpcResponse = response.json().await.map_err(|_| ApiError::Protocol)?;
        if response.id != id {
            return Err(ApiError::Protocol);
        }
        if let Some(error) = response.error {
            if error.message == "MESSAGE_METHOD_UNKNOWN" {
                return Err(ApiError::Unsupported);
            }
            if error.message == "HOST_IS_SLAVE" {
                let master = error
                    .data
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                return Err(ApiError::HostIsSlave(master));
            }
            let message = match error.message.as_str() {
                "SESSION_AUTHENTICATION_FAILED" => {
                    "Authentication failed. Check your username and password."
                }
                "SESSION_INVALID" => "The session has expired. Reconnect to the host.",
                "HOST_IS_SLAVE" => "This host is a pool member. Connect to the pool coordinator.",
                "RBAC_PERMISSION_DENIED" => {
                    "Your account does not have permission to read this resource."
                }
                "HANDLE_INVALID" => {
                    "The object no longer exists on the host. Refresh and try again."
                }
                "VM_BAD_POWER_STATE" => {
                    "The VM is not in the power state required for this operation."
                }
                "VM_HAS_SNAPSHOTS" => {
                    "The VM still has snapshots. Delete them before deleting the VM."
                }
                "VM_IS_DEFAULT_TEMPLATE" => "The default template cannot be deleted.",
                "VDI_IN_USE" => "The disk is still attached to a VM and cannot be deleted.",
                "OPERATION_NOT_ALLOWED" => {
                    "The host does not allow this operation in the object's current state."
                }
                "SR_FULL" => "The storage repository does not have enough free space.",
                "HOST_OFFLINE" => "The host is offline. Reconnect it and try again.",
                _ => "The XenAPI request failed.",
            };
            return Err(ApiError::XenCode(error.message, message.into()));
        }
        serde_json::from_value(response.result).map_err(|_| ApiError::Protocol)
    }

    pub(crate) async fn records(&self, class: &str) -> Result<HashMap<String, Value>, ApiError> {
        self.request(&format!("{class}.get_all_records"), json!([self.session]))
            .await
    }

    /// Lightweight liveness check: read the pool record, then ask the
    /// coordinator for its clock. Both requests are cheap and exercise the
    /// same session used by normal operations.
    pub async fn heartbeat(&self) -> Result<(), ApiError> {
        let pools = self.records("pool").await?;
        let master = pools
            .values()
            .next()
            .and_then(|pool| pool["master"].as_str())
            .filter(|master| !master.is_empty() && *master != "OpaqueRef:NULL");
        match master {
            Some(master) => self
                .request::<Value>("host.get_servertime", json!([self.session, master]))
                .await
                .map(|_| ()),
            None => self
                .request::<Value>("pool.get_all", json!([self.session]))
                .await
                .map(|_| ()),
        }
    }

    pub async fn logout(&self) -> Result<(), ApiError> {
        self.request::<Value>("session.logout", json!([self.session]))
            .await
            .map(|_| ())
    }

    pub async fn inventory(&self) -> Result<Inventory, ApiError> {
        let raw = self.raw_inventory().await?;
        raw.into_inventory(self.settings.clone())
    }

    pub async fn raw_inventory(&self) -> Result<RawInventory, ApiError> {
        let (
            pools,
            hosts,
            metrics,
            vms,
            storage,
            networks,
            tasks,
            messages,
            vbds,
            vdis,
            pbds,
            guest_metrics,
            pifs,
            bonds,
            vifs,
            certificates,
        ) = tokio::join!(
            self.records("pool"),
            self.records("host"),
            self.records("host_metrics"),
            self.records("VM"),
            self.records("SR"),
            self.records("network"),
            self.records("task"),
            self.records("message"),
            self.records("VBD"),
            self.records("VDI"),
            self.records("PBD"),
            self.records("VM_guest_metrics"),
            self.records("PIF"),
            self.records("Bond"),
            self.records("VIF"),
            self.records("Certificate")
        );
        Ok(RawInventory {
            pools: pools?,
            hosts: hosts?,
            host_metrics: metrics?,
            vms: vms?,
            storage: storage?,
            networks: networks?,
            // Older hosts may not expose task listing to this role; the
            // inventory remains useful without it.
            tasks: tasks.unwrap_or_default(),
            messages: messages.unwrap_or_default(),
            vbds: vbds.unwrap_or_default(),
            vdis: vdis.unwrap_or_default(),
            pbds: pbds.unwrap_or_default(),
            guest_metrics: guest_metrics.unwrap_or_default(),
            pifs: pifs.unwrap_or_default(),
            bonds: bonds.unwrap_or_default(),
            vifs: vifs.unwrap_or_default(),
            certificates: certificates.unwrap_or_default(),
        })
    }

    pub async fn event_register(&self, classes: &[&str]) -> Result<(), ApiError> {
        self.request::<Value>("event.register", json!([self.session, classes]))
            .await
            .map(|_| ())
    }

    pub async fn event_next(&self) -> Result<Vec<Value>, ApiError> {
        self.request("event.next", json!([self.session])).await
    }

    pub async fn event_unregister(&self, classes: &[&str]) -> Result<(), ApiError> {
        self.request::<Value>("event.unregister", json!([self.session, classes]))
            .await
            .map(|_| ())
    }
}

#[derive(Clone, Default)]
pub struct RawInventory {
    pub(crate) pools: HashMap<String, Value>,
    pub(crate) hosts: HashMap<String, Value>,
    pub(crate) host_metrics: HashMap<String, Value>,
    pub(crate) vms: HashMap<String, Value>,
    pub(crate) storage: HashMap<String, Value>,
    pub(crate) networks: HashMap<String, Value>,
    pub(crate) tasks: HashMap<String, Value>,
    pub(crate) messages: HashMap<String, Value>,
    pub(crate) vbds: HashMap<String, Value>,
    pub(crate) vdis: HashMap<String, Value>,
    pub(crate) pbds: HashMap<String, Value>,
    pub(crate) guest_metrics: HashMap<String, Value>,
    pub(crate) pifs: HashMap<String, Value>,
    pub(crate) bonds: HashMap<String, Value>,
    pub(crate) vifs: HashMap<String, Value>,
    pub(crate) certificates: HashMap<String, Value>,
}

impl RawInventory {
    /// Apply a batch of XenAPI events and report whether any inventory class changed.
    pub fn apply_events(&mut self, events: &[Value]) -> bool {
        let mut changed = false;
        for event in events {
            let class = event["class"].as_str().unwrap_or_default();
            let reference = event["ref"].as_str().unwrap_or_default();
            let operation = event["operation"].as_str().unwrap_or_default();
            // XAPI emits lower-case class names in events (`vm`, `sr`) even
            // though the API class names are mixed case.
            let target = match class.to_ascii_lowercase().as_str() {
                "pool" => &mut self.pools,
                "host" => &mut self.hosts,
                "host_metrics" => &mut self.host_metrics,
                "vm" => &mut self.vms,
                "sr" => &mut self.storage,
                "network" => &mut self.networks,
                "task" => &mut self.tasks,
                "message" => &mut self.messages,
                "vbd" => &mut self.vbds,
                "vdi" => &mut self.vdis,
                "pbd" => &mut self.pbds,
                "vm_guest_metrics" => &mut self.guest_metrics,
                "pif" => &mut self.pifs,
                "bond" => &mut self.bonds,
                "vif" => &mut self.vifs,
                "certificate" => &mut self.certificates,
                _ => continue,
            };
            if operation == "del" {
                target.remove(reference);
                changed = true;
            } else if let Some(snapshot) = event.get("snapshot").filter(|value| value.is_object()) {
                target.insert(reference.to_owned(), snapshot.clone());
                changed = true;
            }
        }
        let excess = self.tasks.len().saturating_sub(500);
        if excess > 0 {
            let mut oldest: Vec<(String, String)> = self
                .tasks
                .iter()
                .map(|(reference, task)| (reference.clone(), text(task, "created")))
                .collect();
            oldest.sort_by(|left, right| left.1.cmp(&right.1));
            for (reference, _) in oldest.into_iter().take(excess) {
                self.tasks.remove(&reference);
            }
        }
        changed
    }

    pub fn into_inventory(&self, settings: ConnectionSettings) -> Result<Inventory, ApiError> {
        let mut hosts: Vec<_> = self
            .hosts
            .iter()
            .map(|(reference, r)| {
                let metric = self
                    .host_metrics
                    .get(&text(r, "metrics"))
                    .unwrap_or(&Value::Null);
                Host {
                    reference: reference.clone(),
                    uuid: text(r, "uuid"),
                    name: text(r, "name_label"),
                    address: text(r, "address"),
                    version: text(&r["software_version"], "product_version"),
                    brand: text(&r["software_version"], "product_brand"),
                    enabled: boolean(r, "enabled"),
                    cpu_count: r["host_CPUs"].as_array().map_or(0, Vec::len),
                    memory_total: number(metric, "memory_total"),
                    memory_free: number(metric, "memory_free"),
                }
            })
            .collect();
        hosts.sort_by(|a, b| a.name.cmp(&b.name));
        let host_names: HashMap<_, _> = hosts
            .iter()
            .map(|h| (h.reference.clone(), h.name.clone()))
            .collect();
        let all_vms: Vec<_> = self
            .vms
            .iter()
            .filter(|(_, r)| !boolean(r, "is_control_domain"))
            .map(|(reference, r)| vm_record(reference.clone(), r, &host_names))
            .collect();
        let mut vms: Vec<_> = all_vms.iter().filter(|r| r.kind == "vm").cloned().collect();
        let mut templates: Vec<_> = all_vms
            .iter()
            .filter(|r| r.kind == "template")
            .cloned()
            .collect();
        let mut snapshots: Vec<_> = all_vms
            .into_iter()
            .filter(|r| r.kind == "snapshot")
            .collect();
        vms.sort_by(|a, b| a.name.cmp(&b.name));
        templates.sort_by(|a, b| a.name.cmp(&b.name));
        snapshots.sort_by(|a, b| b.snapshot_time.cmp(&a.snapshot_time));
        let mut storage: Vec<_> = self
            .storage
            .iter()
            .map(|(reference, r)| {
                let pbd_refs = r["PBDs"].as_array().cloned().unwrap_or_default();
                let pbd_count = pbd_refs.len();
                let pbd_attached = pbd_refs
                    .iter()
                    .filter(|reference| {
                        reference.as_str().is_some_and(|reference| {
                            self.pbds.get(reference).is_some_and(|pbd| {
                                pbd["currently_attached"].as_bool() == Some(true)
                            })
                        })
                    })
                    .count();
                Storage {
                    uuid: text(r, "uuid"),
                    name: text(r, "name_label"),
                    storage_type: text(r, "type"),
                    content_type: text(r, "content_type"),
                    description: text(r, "name_description"),
                    total: number(r, "physical_size"),
                    used: number(r, "physical_utilisation"),
                    shared: boolean(r, "shared"),
                    pbd_count,
                    pbd_attached,
                    is_default: self
                        .pools
                        .values()
                        .any(|pool| pool["default_SR"].as_str() == Some(reference.as_str())),
                    can_be_default: r["content_type"].as_str() != Some("iso")
                        && pbd_attached > 0
                        && (boolean(r, "shared") || self.hosts.len() <= 1)
                        && r["allowed_operations"].as_array().is_some_and(|ops| {
                            ops.iter().any(|op| op.as_str() == Some("vdi_create"))
                        }),
                }
            })
            .collect();
        storage.sort_by(|a, b| a.name.cmp(&b.name));
        let mut networks: Vec<_> = self
            .networks
            .values()
            .map(|r| Network {
                uuid: text(r, "uuid"),
                name: text(r, "name_label"),
                description: text(r, "name_description"),
                bridge: text(r, "bridge"),
                automatic: text(&r["other_config"], "automatic") != "false",
                mtu: number(r, "MTU"),
                interface_count: r["PIFs"].as_array().map_or(0, Vec::len),
                virtual_interface_count: r["VIFs"].as_array().map_or(0, Vec::len),
            })
            .collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));
        let pool_name = self
            .pools
            .values()
            .next()
            .map(|r| text(r, "name_label"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                hosts
                    .first()
                    .map_or("Pool".into(), |h| format!("{} pool", h.name))
            });
        let pool_id = self
            .pools
            .values()
            .next()
            .map(|r| text(r, "uuid"))
            .filter(|s| !s.is_empty())
            .ok_or(ApiError::Protocol)?;
        let mut tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|(_, task)| !text(task, "name_label").is_empty())
            .map(|(reference, task)| {
                let host_ref = text(task, "resident_on");
                ExternalTask {
                    reference: reference.clone(),
                    uuid: text(task, "uuid"),
                    name: text(task, "name_label"),
                    status: text(task, "status"),
                    progress: task["progress"]
                        .as_f64()
                        .or_else(|| task["progress"].as_str().and_then(|p| p.parse().ok()))
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0),
                    host_name: host_names.get(&host_ref).cloned().unwrap_or_default(),
                    task_type: text(task, "type"),
                    started_at: xapi_epoch(&text(task, "created")),
                    finished_at: xapi_epoch(&text(task, "finished")),
                    error: task["error_info"]
                        .as_array()
                        .and_then(|values| values.first())
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    can_cancel: task["allowed_operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("cancel"))),
                }
            })
            .collect();
        tasks.sort_by_key(|task| std::cmp::Reverse(task.started_at));
        tasks.truncate(100);
        let mut alerts: Vec<_> = self
            .messages
            .iter()
            .filter(|(_, message)| {
                message["priority"]
                    .as_u64()
                    .or_else(|| message["priority"].as_str().and_then(|p| p.parse().ok()))
                    .is_some_and(|priority| (1..=4).contains(&priority))
            })
            .map(|(reference, message)| PoolAlert {
                reference: reference.clone(),
                uuid: text(message, "uuid"),
                name: text(message, "name"),
                priority: message["priority"]
                    .as_u64()
                    .or_else(|| message["priority"].as_str().and_then(|p| p.parse().ok()))
                    .unwrap_or(5),
                time: xapi_epoch(&text(message, "timestamp")),
                body: text(message, "body"),
                obj_uuid: text(message, "obj_uuid"),
                class: text(message, "cls"),
            })
            .collect();
        alerts.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(right.time.cmp(&left.time))
        });
        alerts.truncate(100);
        let mut navigation = HashMap::new();
        for records in [
            &self.pools,
            &self.hosts,
            &self.vms,
            &self.storage,
            &self.networks,
            &self.vdis,
        ] {
            for r in records.values() {
                let other = &r["other_config"];
                let mut home = String::new();
                if r.get("power_state").is_some() && !boolean(r, "is_a_template") {
                    if matches!(text(r, "power_state").as_str(), "Running" | "Paused") {
                        home = text(r, "resident_on");
                    } else {
                        let storage_host = r["VBDs"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|vbd| self.vbds.get(vbd.as_str().unwrap_or_default()))
                            .filter_map(|vbd| self.vdis.get(&text(vbd, "VDI")))
                            .filter_map(|vdi| self.storage.get(&text(vdi, "SR")))
                            .filter(|sr| !boolean(sr, "shared"))
                            .filter_map(|sr| sr["PBDs"].as_array().filter(|pbds| pbds.len() == 1))
                            .filter_map(|pbds| self.pbds.get(pbds[0].as_str().unwrap_or_default()))
                            .map(|pbd| text(pbd, "host"))
                            .find(|host| self.hosts.contains_key(host));
                        if let Some(host) = storage_host {
                            home = host;
                        } else {
                            let affinity = text(r, "affinity");
                            if self
                                .hosts
                                .get(&affinity)
                                .and_then(|h| self.host_metrics.get(&text(h, "metrics")))
                                .is_some_and(|m| boolean(m, "live"))
                            {
                                home = affinity;
                            }
                        }
                    }
                } else if r.get("PBDs").is_some()
                    && !boolean(r, "shared")
                    && let Some(pbds) = r["PBDs"].as_array().filter(|p| p.len() == 1)
                {
                    home = self
                        .pbds
                        .get(pbds[0].as_str().unwrap_or_default())
                        .map_or_else(String::new, |p| text(p, "host"));
                }
                navigation.insert(
                    text(r, "uuid"),
                    NavigationMetadata {
                        home,
                        hidden: text(other, "HideFromXenCenter").eq_ignore_ascii_case("true"),
                        default_template: other.get("disks").is_some(),
                        tools_sr: text(other, "xenserver_tools_sr").eq_ignore_ascii_case("true"),
                        live: self
                            .host_metrics
                            .get(&text(r, "metrics"))
                            .is_some_and(|m| boolean(m, "live")),
                        folder: text(other, "folder"),
                        custom_fields: other
                            .as_object()
                            .into_iter()
                            .flatten()
                            .filter_map(|(k, v)| {
                                k.strip_prefix("XenCenter.CustomFields.").and_then(|key| {
                                    v.as_str().map(|value| (key.to_owned(), value.to_owned()))
                                })
                            })
                            .collect(),
                        tags: r["tags"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                    },
                );
            }
        }
        for vdi in self.vdis.values() {
            let home = self
                .storage
                .get(&text(vdi, "SR"))
                .and_then(|sr| navigation.get(&text(sr, "uuid")))
                .map(|m| m.home.clone())
                .unwrap_or_default();
            if let Some(metadata) = navigation.get_mut(&text(vdi, "uuid")) {
                metadata.home = home;
            }
        }
        let named_pool = self
            .pools
            .values()
            .any(|p| !text(p, "name_label").is_empty());
        let mut virtual_disks: Vec<_> = self
            .vdis
            .values()
            .map(|vdi| {
                let sr = self.storage.get(&text(vdi, "SR")).unwrap_or(&Value::Null);
                VirtualDisk {
                    uuid: text(vdi, "uuid"),
                    name: text(vdi, "name_label"),
                    description: text(vdi, "name_description"),
                    size: number(vdi, "virtual_size"),
                    sr_uuid: text(sr, "uuid"),
                    sr_name: text(sr, "name_label"),
                    read_only: boolean(vdi, "read_only"),
                }
            })
            .collect();
        virtual_disks.sort_by(|a, b| a.name.cmp(&b.name).then(a.uuid.cmp(&b.uuid)));
        Ok(Inventory {
            virtual_disks,
            navigation,
            named_pool,
            pool_id,
            templates,
            snapshots,
            settings,
            pool_name,
            hosts,
            vms,
            storage,
            networks,
            tasks,
            alerts,
            health: crate::health::evaluate(self),
        })
    }
}

fn xapi_epoch(value: &str) -> u64 {
    chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H:%M:%SZ")
        .map(|date| date.and_utc().timestamp().max(0) as u64)
        .unwrap_or(0)
}

fn vm_record(reference: String, r: &Value, host_names: &HashMap<String, String>) -> VirtualMachine {
    let resident_on = text(r, "resident_on");
    let host_ref = if resident_on == "OpaqueRef:NULL" || resident_on.is_empty() {
        text(r, "affinity")
    } else {
        resident_on.clone()
    };
    VirtualMachine {
        reference,
        kind: if boolean(r, "is_a_snapshot") {
            "snapshot"
        } else if boolean(r, "is_a_template") {
            "template"
        } else {
            "vm"
        }
        .into(),
        allowed_operations: r["allowed_operations"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        host_name: host_names
            .get(&host_ref)
            .cloned()
            .unwrap_or_else(|| "Unassigned".into()),
        resident_on: host_ref,
        snapshot_of: text(r, "snapshot_of"),
        snapshot_time: text(r, "snapshot_time"),
        uuid: text(r, "uuid"),
        name: text(r, "name_label"),
        description: text(r, "name_description"),
        power_state: text(r, "power_state"),
        vcpus: number(r, "VCPUs_max"),
        memory: number(r, "memory_dynamic_max"),
        os: text(&r["other_config"], "os_type"),
        tags: r["tags"]
            .as_array()
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        disk_count: r["VBDs"].as_array().map_or(0, Vec::len),
        interface_count: r["VIFs"].as_array().map_or(0, Vec::len),
    }
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

fn boolean(record: &Value, key: &str) -> bool {
    record[key].as_bool().unwrap_or(false)
}

fn number(record: &Value, key: &str) -> u64 {
    let value = &record[key];
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|n| n.parse().ok()))
        .unwrap_or(0)
}

#[derive(Clone, Serialize)]
pub struct VirtualDisk {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub size: u64,
    pub sr_uuid: String,
    pub sr_name: String,
    pub read_only: bool,
}

#[derive(Clone, Serialize)]
pub struct Inventory {
    pub virtual_disks: Vec<VirtualDisk>,
    pub navigation: HashMap<String, NavigationMetadata>,
    pub named_pool: bool,
    pub settings: ConnectionSettings,
    pub pool_id: String,
    pub templates: Vec<VirtualMachine>,
    pub snapshots: Vec<VirtualMachine>,
    pub pool_name: String,
    pub hosts: Vec<Host>,
    pub vms: Vec<VirtualMachine>,
    pub storage: Vec<Storage>,
    pub networks: Vec<Network>,
    pub tasks: Vec<ExternalTask>,
    pub alerts: Vec<PoolAlert>,
    pub health: crate::health::HealthReport,
}

/// Navigation-only projection of records already fetched by raw_inventory.
#[derive(Clone, Default, Serialize)]
pub struct NavigationMetadata {
    pub home: String,
    pub hidden: bool,
    pub default_template: bool,
    pub tools_sr: bool,
    pub live: bool,
    pub folder: String,
    pub custom_fields: HashMap<String, String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct Host {
    pub reference: String,
    pub uuid: String,
    pub name: String,
    pub address: String,
    pub version: String,
    pub brand: String,
    pub enabled: bool,
    pub cpu_count: usize,
    pub memory_total: u64,
    pub memory_free: u64,
}

#[derive(Clone, Serialize)]
pub struct VirtualMachine {
    pub reference: String,
    pub kind: String,
    pub allowed_operations: Vec<String>,
    pub host_name: String,
    pub resident_on: String,
    pub snapshot_of: String,
    pub snapshot_time: String,
    pub disk_count: usize,
    pub interface_count: usize,
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub power_state: String,
    pub vcpus: u64,
    pub memory: u64,
    pub os: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct PoolAlert {
    pub reference: String,
    pub uuid: String,
    pub name: String,
    pub priority: u64,
    pub time: u64,
    pub body: String,
    pub obj_uuid: String,
    pub class: String,
}

#[derive(Clone, Serialize)]
pub struct ExternalTask {
    pub reference: String,
    pub uuid: String,
    pub name: String,
    pub status: String,
    pub progress: f64,
    pub host_name: String,
    pub task_type: String,
    pub started_at: u64,
    pub finished_at: u64,
    pub error: String,
    pub can_cancel: bool,
}

#[derive(Clone, Serialize)]
pub struct Storage {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub storage_type: String,
    pub content_type: String,
    pub total: u64,
    pub used: u64,
    pub shared: bool,
    pub pbd_count: usize,
    pub pbd_attached: usize,
    pub is_default: bool,
    pub can_be_default: bool,
}

#[derive(Clone, Serialize)]
pub struct Network {
    pub automatic: bool,
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub bridge: String,
    pub mtu: u64,
    pub interface_count: usize,
    pub virtual_interface_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_projects_existing_records_without_new_fetches() {
        let mut raw = RawInventory::default();
        raw.hosts.insert(
            "h".into(),
            json!({"uuid":"host", "metrics":"hm", "name_label":"server", "enabled":true}),
        );
        raw.host_metrics.insert("hm".into(), json!({"live":true}));
        raw.pools
            .insert("p".into(), json!({"uuid":"pool", "name_label":""}));
        raw.pbds.insert("pbd".into(), json!({"host":"h"}));
        raw.storage.insert(
            "sr".into(),
            json!({"uuid":"local", "shared":false,"PBDs":["pbd"]}),
        );
        raw.vdis.insert("vdi".into(), json!({"SR":"sr"}));
        raw.vbds
            .insert("vbd".into(), json!({"VM":"vm", "VDI":"vdi"}));
        raw.vms.insert("vm".into(), json!({"uuid":"vm", "power_state":"Halted", "VBDs":["vbd"], "other_config":{"HideFromXenCenter":"true","folder":"/one/two","XenCenter.CustomFields.Owner":"Team"},"tags":["prod"]}));
        raw.vms.insert(
            "template".into(),
            json!({"uuid":"template","is_a_template":true,"other_config":{"disks":"xml"}}),
        );
        let settings =
            serde_json::from_value(json!({"host":"example.test","username":"root"})).unwrap();
        let inventory = raw.into_inventory(settings).unwrap();
        assert!(!inventory.named_pool);
        assert_eq!(inventory.navigation["vm"].home, "h");
        assert_eq!(inventory.navigation["local"].home, "h");
        assert!(inventory.navigation["host"].live);
        assert!(inventory.navigation["vm"].hidden);
        assert_eq!(inventory.navigation["vm"].custom_fields["Owner"], "Team");
        assert_eq!(inventory.navigation["vm"].folder, "/one/two");
        assert_eq!(inventory.navigation["vm"].tags, vec!["prod"]);
        assert!(inventory.navigation["template"].default_template);
        assert!(inventory.navigation["template"].home.is_empty());
    }

    #[test]
    fn xenapi_numbers_accept_strings_and_unknown_capacity() {
        let record = json!({"string":"4096","number":8192,"unknown":-1});
        assert_eq!(number(&record, "string"), 4096);
        assert_eq!(number(&record, "number"), 8192);
        assert_eq!(number(&record, "unknown"), 0);
    }
    #[test]
    fn proxy_urls_are_validated_per_connection() {
        let mut settings: ConnectionSettings = serde_json::from_value(json!({
            "host": "example.test",
            "username": "root"
        }))
        .unwrap();
        assert!(settings.proxy_url().unwrap().is_none());
        settings.proxy = Some("http://proxy.example:3128".into());
        assert_eq!(settings.proxy_url().unwrap().unwrap().scheme(), "http");
        settings.proxy = Some("socks5://127.0.0.1:1080".into());
        assert!(settings.proxy_url().is_ok());
        for invalid in [
            "proxy.example:3128",
            "ftp://proxy.example:21",
            "http://proxy.example:3128/path",
            "http://proxy.example:3128/?token=1",
        ] {
            settings.proxy = Some(invalid.into());
            assert!(settings.proxy_url().is_err(), "{invalid} should fail");
        }
    }

    #[test]
    fn xapi_timestamps_are_parsed() {
        assert!(xapi_epoch("20261110T18:36:56Z") > 0);
        assert_eq!(xapi_epoch("19700101T00:00:00Z"), 0);
        assert_eq!(xapi_epoch(""), 0);
    }
    #[test]
    fn production_host_cannot_disable_read_only() {
        let mut settings: ConnectionSettings = serde_json::from_value(
            json!({"host":"10.100.1.11","username":"root","read_only":false}),
        )
        .unwrap();
        settings.enforce_host_policy(&Url::parse("https://10.100.1.11:443").unwrap());
        assert!(settings.read_only);
    }

    #[tokio::test]
    async fn read_only_blocks_mutations_before_any_network_request() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut settings: ConnectionSettings = serde_json::from_value(
            json!({"host":"10.100.1.11","username":"root","read_only":true}),
        )
        .unwrap();
        settings.enforce_host_policy(&Url::parse("https://10.100.1.11").unwrap());
        let connection = Connection {
            transport: HostTransport::new(
                Url::parse(&format!("https://{}", listener.local_addr().unwrap())).unwrap(),
                TlsOptions::default(),
                None,
            )
            .unwrap(),
            session: "test-session".into(),
            next_id: AtomicU64::new(1),
            settings,
        };
        let error = connection
            .begin_vm_action("test-vm", &crate::operations::VmAction::Start)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("read-only"));
        let error = connection.cancel_task("test-task").await.unwrap_err();
        assert!(error.to_string().contains("read-only"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
