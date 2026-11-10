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
    #[error("{1}")]
    XenCode(String, String),
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
}

impl ConnectionSettings {
    fn enforce_host_policy(&mut self, origin: &Url) {
        if origin.host_str() == Some("10.100.1.11") {
            self.read_only = true;
        }
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
}

pub struct Connection {
    pub(crate) transport: HostTransport,
    pub(crate) session: String,
    next_id: AtomicU64,
    pub settings: ConnectionSettings,
}

impl Connection {
    pub async fn login(mut settings: ConnectionSettings, password: &str) -> Result<Self, ApiError> {
        let address = if settings.host.contains("://") {
            settings.host.clone()
        } else {
            format!("https://{}", settings.host)
        };
        let origin = Url::parse(&address).map_err(|_| ApiError::InvalidHost)?;
        settings.enforce_host_policy(&origin);
        let transport =
            HostTransport::new(origin, settings.tls).map_err(|_| ApiError::InvalidHost)?;
        let mut connection = Self {
            transport,
            session: String::new(),
            next_id: AtomicU64::new(1),
            settings,
        };
        connection.session = connection
            .request(
                "session.login_with_password",
                json!([
                    connection.settings.username,
                    password,
                    "1.0",
                    "Xen Desktop/0.1"
                ]),
            )
            .await?;
        Ok(connection)
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

    pub async fn logout(&self) -> Result<(), ApiError> {
        self.request::<Value>("session.logout", json!([self.session]))
            .await
            .map(|_| ())
    }

    pub async fn inventory(&self) -> Result<Inventory, ApiError> {
        let pools = self.records("pool").await?;
        let hosts = self.records("host").await?;
        let metrics = self.records("host_metrics").await?;
        let vms = self.records("VM").await?;
        let storage = self.records("SR").await?;
        let networks = self.records("network").await?;
        let mut hosts: Vec<_> = hosts
            .into_iter()
            .map(|(reference, r)| {
                let metric = metrics.get(&text(&r, "metrics")).unwrap_or(&Value::Null);
                Host {
                    reference,
                    uuid: text(&r, "uuid"),
                    name: text(&r, "name_label"),
                    address: text(&r, "address"),
                    version: text(&r["software_version"], "product_version"),
                    brand: text(&r["software_version"], "product_brand"),
                    enabled: boolean(&r, "enabled"),
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
        let all_vms: Vec<_> = vms
            .into_iter()
            .filter(|(_, r)| !boolean(r, "is_control_domain"))
            .map(|(reference, r)| vm_record(reference, &r, &host_names))
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
        let mut storage: Vec<_> = storage
            .into_values()
            .map(|r| Storage {
                uuid: text(&r, "uuid"),
                name: text(&r, "name_label"),
                storage_type: text(&r, "type"),
                description: text(&r, "name_description"),
                total: number(&r, "physical_size"),
                used: number(&r, "physical_utilisation"),
                shared: boolean(&r, "shared"),
            })
            .collect();
        storage.sort_by(|a, b| a.name.cmp(&b.name));
        let mut networks: Vec<_> = networks
            .into_values()
            .map(|r| Network {
                uuid: text(&r, "uuid"),
                name: text(&r, "name_label"),
                description: text(&r, "name_description"),
                bridge: text(&r, "bridge"),
                mtu: number(&r, "MTU"),
                interface_count: r["PIFs"].as_array().map_or(0, Vec::len),
                virtual_interface_count: r["VIFs"].as_array().map_or(0, Vec::len),
            })
            .collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));
        let pool_name = pools
            .values()
            .next()
            .map(|r| text(r, "name_label"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                hosts
                    .first()
                    .map_or("Pool".into(), |h| format!("{} pool", h.name))
            });
        let pool_id = pools
            .values()
            .next()
            .map(|r| text(r, "uuid"))
            .filter(|s| !s.is_empty())
            .ok_or(ApiError::Protocol)?;
        Ok(Inventory {
            pool_id,
            templates,
            snapshots,
            settings: self.settings.clone(),
            pool_name,
            hosts,
            vms,
            storage,
            networks,
        })
    }
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
pub struct Inventory {
    pub settings: ConnectionSettings,
    pub pool_id: String,
    pub templates: Vec<VirtualMachine>,
    pub snapshots: Vec<VirtualMachine>,
    pub pool_name: String,
    pub hosts: Vec<Host>,
    pub vms: Vec<VirtualMachine>,
    pub storage: Vec<Storage>,
    pub networks: Vec<Network>,
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
}

#[derive(Clone, Serialize)]
pub struct Storage {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub storage_type: String,
    pub total: u64,
    pub used: u64,
    pub shared: bool,
}

#[derive(Clone, Serialize)]
pub struct Network {
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
    fn xenapi_numbers_accept_strings_and_unknown_capacity() {
        let record = json!({"string":"4096","number":8192,"unknown":-1});
        assert_eq!(number(&record, "string"), 4096);
        assert_eq!(number(&record, "number"), 8192);
        assert_eq!(number(&record, "unknown"), 0);
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
