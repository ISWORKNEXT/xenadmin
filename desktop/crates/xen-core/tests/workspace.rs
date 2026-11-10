use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tempfile::TempDir;
use xen_core::{
    operations::{TaskStatus, VmAction},
    workspace::{ConnectionView, Workspace},
    xapi::ConnectionSettings,
};

struct Fixture {
    child: Child,
    _files: TempDir,
    settings: ConnectionSettings,
}
impl Fixture {
    fn new(pool: &str) -> Self {
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
        let mut child = Command::new("python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixture.py"))
            .arg(pool)
            .arg(&cert)
            .arg(&key)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port: u16 = line.trim().parse().unwrap();
        let settings=serde_json::from_value(serde_json::json!({"host":format!("127.0.0.1:{port}"),"username":"fixture","label":pool,"tls":{"bypass_certificate_verification":true}})).unwrap();
        Self {
            child,
            _files: files,
            settings,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
async fn connect(workspace: &Arc<Workspace>, fixture: &Fixture) -> String {
    workspace
        .connect(fixture.settings.clone(), "fixture-password", None)
        .await
        .unwrap()
}
async fn terminal(workspace: &Arc<Workspace>, id: &str) -> TaskStatus {
    for _ in 0..100 {
        let state = workspace.view().await;
        if let Some(task) = state.tasks.iter().find(|t| t.id == id)
            && task.status.terminal()
        {
            return task.status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("task did not finish")
}

#[tokio::test]
async fn same_vm_uuid_and_name_in_two_pools_never_cross_targets() {
    let a = Fixture::new("pool-a");
    let b = Fixture::new("pool-b");
    let workspace = Workspace::with_profiles(vec![]);
    let aid = connect(&workspace, &a).await;
    let bid = connect(&workspace, &b).await;
    let task = workspace
        .start_action(aid.clone(), "same-vm-uuid".into(), VmAction::Start)
        .await
        .unwrap();
    assert!(terminal(&workspace, &task.id).await == TaskStatus::Succeeded);
    // Explicit refresh also waits for the final host state instead of trusting a task label.
    workspace.refresh(&aid).await.unwrap();
    workspace.refresh(&bid).await.unwrap();
    let view = workspace.view().await;
    for c in view.connections {
        if let ConnectionView::Online { id, inventory, .. } = c {
            assert_eq!(
                inventory.vms[0].power_state,
                if id == aid { "Running" } else { "Halted" }
            );
        }
    }
    workspace.disconnect(&aid).await.unwrap();
    assert!(
        workspace
            .view()
            .await
            .connections
            .iter()
            .any(|c| matches!(c,ConnectionView::Online {id,..} if id==&bid))
    );
    workspace.close().await;
}
#[tokio::test]
async fn host_failure_is_reported_as_failure_not_success() {
    let fixture = Fixture::new("failure-pool");
    let workspace = Workspace::with_profiles(vec![]);
    let id = connect(&workspace, &fixture).await;
    let task = workspace
        .start_action(id, "same-vm-uuid".into(), VmAction::Start)
        .await
        .unwrap();
    assert!(terminal(&workspace, &task.id).await == TaskStatus::Failed);
    assert_eq!(
        workspace.view().await.tasks[0].message,
        "NOT_ENOUGH_FREE_MEMORY"
    );
    workspace.close().await;
}
#[tokio::test]
async fn cancellation_uses_the_original_connection_and_task() {
    let fixture = Fixture::new("cancel-pool");
    let workspace = Workspace::with_profiles(vec![]);
    let id = connect(&workspace, &fixture).await;
    let task = workspace
        .start_action(id, "same-vm-uuid".into(), VmAction::Start)
        .await
        .unwrap();
    for _ in 0..100 {
        if workspace.view().await.tasks[0].can_cancel {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    workspace.cancel_task(&task.id).await.unwrap();
    assert!(terminal(&workspace, &task.id).await == TaskStatus::Cancelled);
    workspace.close().await;
}
#[tokio::test]
async fn failed_pool_refresh_preserves_other_connections() {
    let mut a = Fixture::new("pool-a");
    let b = Fixture::new("pool-b");
    let workspace = Workspace::with_profiles(vec![]);
    let aid = connect(&workspace, &a).await;
    let bid = connect(&workspace, &b).await;
    a.child.kill().unwrap();
    a.child.wait().unwrap();
    assert!(workspace.refresh(&aid).await.is_err());
    workspace.refresh(&bid).await.unwrap();
    let view = workspace.view().await;
    assert!(
        view.connections
            .iter()
            .any(|c| matches!(c,ConnectionView::Stale{id,..} if id==&aid))
    );
    assert!(
        view.connections
            .iter()
            .any(|c| matches!(c,ConnectionView::Online{id,..} if id==&bid))
    );
    workspace.close().await;
}

#[tokio::test]
async fn host_detail_requests_cannot_escape_the_selected_connection() {
    let first = Fixture::new("pool-first");
    let second = Fixture::new("pool-second");
    let workspace = Workspace::with_profiles(vec![]);
    let first_id = connect(&workspace, &first).await;
    let second_id = connect(&workspace, &second).await;

    // These fixtures deliberately have no host-detail RPC support. The workspace
    // must reject the request before dispatch rather than ask the wrong server.
    let error = workspace
        .host_details(&first_id, "pool-second-host")
        .await
        .unwrap_err();
    assert_eq!(error, "This host is not part of the selected connection.");
    let error = workspace.host_details(&second_id, "").await.unwrap_err();
    assert_eq!(error, "This host is not part of the selected connection.");
    let error = workspace
        .host_details("unknown-connection", "pool-first-host")
        .await
        .unwrap_err();
    assert!(error.contains("offline"));
    workspace.close().await;
}
