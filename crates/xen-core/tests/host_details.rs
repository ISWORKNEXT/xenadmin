use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

use tempfile::TempDir;
use xen_core::{
    host::DataSection,
    xapi::{Connection, ConnectionSettings},
};

struct Fixture {
    child: Child,
    files: TempDir,
    settings: ConnectionSettings,
}

impl Fixture {
    fn new(pool: &str, failure: &str) -> Self {
        let files = tempfile::tempdir().unwrap();
        let cert = files.path().join("cert.pem");
        let key = files.path().join("key.pem");
        assert!(
            Command::new("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "1",
                    "-subj",
                    "/CN=localhost",
                    "-keyout"
                ])
                .arg(&key)
                .arg("-out")
                .arg(&cert)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );

        let method_log = files.path().join("methods.log");
        let mut child = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/host_fixture.py"
            ))
            .arg(pool)
            .arg(&cert)
            .arg(&key)
            .arg(&method_log)
            .arg(failure)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port: u16 = line.trim().parse().unwrap();
        let settings = serde_json::from_value(serde_json::json!({
            "host": format!("127.0.0.1:{port}"),
            "username": "fixture-user",
            "label": pool,
            "tls": {"bypass_certificate_verification": true}
        }))
        .unwrap();
        Self {
            child,
            files,
            settings,
        }
    }

    fn methods(&self) -> Vec<String> {
        std::fs::read_to_string(self.files.path().join("methods.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn assert_read_only_methods(methods: &[String]) {
    assert!(!methods.is_empty());
    for method in methods {
        let allowed = method == "session.login_with_password"
            || method == "session.logout"
            || method == "host.get_servertime"
            || method == "network_sriov.get_remaining_capacity"
            || method.ends_with(".get_by_uuid")
            || method.ends_with(".get_record")
            || method.ends_with(".get_all_records");
        assert!(allowed, "unexpected non-read-only XenAPI call: {method}");
    }
}

fn memory_field<'a>(details: &'a xen_core::host::HostDetails, label: &str) -> &'a str {
    details
        .memory
        .iter()
        .flat_map(|group| group.fields.iter())
        .find(|field| field.label == label)
        .unwrap_or_else(|| panic!("missing host memory field: {label}"))
        .value
        .as_str()
}

#[tokio::test]
async fn host_details_are_scoped_redacted_and_read_only() {
    let fixture = Fixture::new("host-details", "");
    let connection = Connection::login(fixture.settings.clone(), "fixture-password")
        .await
        .unwrap();

    let details = connection.host_details("host-a-uuid").await.unwrap();
    assert_eq!(details.host_uuid, "host-a-uuid");
    assert_eq!(details.name, "Fixture host A");
    assert_eq!(details.address, "192.0.2.10");
    assert_eq!(memory_field(&details, "Xen and control domain"), "2.00 GiB");
    assert_eq!(memory_field(&details, "Control domain memory"), "1.00 GiB");
    assert_eq!(memory_field(&details, "Resident guest memory"), "2.00 GiB");
    assert_eq!(
        memory_field(&details, "Resident minimum reservation"),
        "1.00 GiB"
    );
    assert_eq!(
        memory_field(&details, "Available for placement"),
        "13.00 GiB"
    );

    let json = serde_json::to_string(&details).unwrap();
    for expected in [
        "Fixture CPU A",
        "192.0.2.10",
        "Fixture SR A",
        "Fixture GPU Vendor",
        "Fixture USB A",
        "AA:BB:CC",
        "HOST_A_ALERT",
    ] {
        assert!(json.contains(expected), "missing host A detail: {expected}");
    }
    for forbidden in [
        "Fixture host B",
        "Fixture CPU B",
        "192.0.2.20",
        "Fixture SR B",
        "Other GPU Vendor",
        "Fixture USB B",
        "DD:EE:FF",
        "HOST_B_ALERT",
        "VM_ALERT_CANARY",
        "Fixture VM",
        "Other VM",
        "CANARY_HOST_POWER_ON",
        "CANARY_EXTERNAL_AUTH",
        "CANARY_AUTH_CONFIGURATION",
        "CANARY_LICENSE",
        "CANARY_PBD_DEVICE_CONFIG",
        "CANARY_HOST_OTHER",
    ] {
        assert!(
            !json.contains(forbidden),
            "unexpected cross-host or private value: {forbidden}"
        );
    }

    assert!(matches!(&details.gpus, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.nics, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.storage, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.pci, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.usb, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.certificates, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(matches!(&details.alerts, DataSection::Ready { rows, .. } if rows.len() == 1));

    connection.logout().await.unwrap();
    assert_read_only_methods(&fixture.methods());
}

#[tokio::test]
async fn permission_denied_is_unavailable_and_not_reported_as_empty() {
    let fixture = Fixture::new("host-denied", "denied:PIF.get_all_records");
    let connection = Connection::login(fixture.settings.clone(), "fixture-password")
        .await
        .unwrap();
    let details = connection.host_details("host-a-uuid").await.unwrap();

    assert!(matches!(&details.nics, DataSection::Unavailable { .. }));
    connection.logout().await.unwrap();
    assert_read_only_methods(&fixture.methods());
}

#[tokio::test]
async fn unknown_optional_api_is_unsupported_and_true_zero_is_ready_empty() {
    let unsupported_fixture = Fixture::new("host-unsupported", "unknown:PGPU.get_all_records");
    let unsupported_connection =
        Connection::login(unsupported_fixture.settings.clone(), "fixture-password")
            .await
            .unwrap();
    let unsupported = unsupported_connection
        .host_details("host-a-uuid")
        .await
        .unwrap();
    assert!(matches!(&unsupported.gpus, DataSection::Unsupported { .. }));
    unsupported_connection.logout().await.unwrap();
    assert_read_only_methods(&unsupported_fixture.methods());

    let empty_fixture = Fixture::new("host-empty", "empty:PUSB.get_all_records");
    let empty_connection = Connection::login(empty_fixture.settings.clone(), "fixture-password")
        .await
        .unwrap();
    let empty = empty_connection.host_details("host-a-uuid").await.unwrap();
    assert!(matches!(&empty.usb, DataSection::Ready { rows, .. } if rows.is_empty()));
    empty_connection.logout().await.unwrap();
    assert_read_only_methods(&empty_fixture.methods());

    let missing_fixture = Fixture::new("host-missing", "missing:PUSB.get_all_records");
    let missing_connection =
        Connection::login(missing_fixture.settings.clone(), "fixture-password")
            .await
            .unwrap();
    let missing = missing_connection
        .host_details("host-a-uuid")
        .await
        .unwrap();
    assert!(matches!(&missing.usb, DataSection::Unavailable { .. }));
    assert!(
        missing
            .warnings
            .iter()
            .any(|warning| warning.section == "USB")
    );
    missing_connection.logout().await.unwrap();
    assert_read_only_methods(&missing_fixture.methods());
}

#[tokio::test]
async fn failed_optional_dataset_keeps_primary_section_and_emits_warning() {
    let fixture = Fixture::new(
        "host-optional-failure",
        "denied:PIF_metrics.get_all_records",
    );
    let connection = Connection::login(fixture.settings.clone(), "fixture-password")
        .await
        .unwrap();
    let details = connection.host_details("host-a-uuid").await.unwrap();

    assert!(matches!(&details.nics, DataSection::Ready { rows, .. } if rows.len() == 1));
    assert!(
        details
            .warnings
            .iter()
            .any(|warning| warning.section == "Networking")
    );
    connection.logout().await.unwrap();
    assert_read_only_methods(&fixture.methods());
}
