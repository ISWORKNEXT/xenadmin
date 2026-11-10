use serde::Deserialize;
use std::io::{self, BufRead};
use xen_core::{
    operations::VmAction,
    workspace::{ConnectionView, Profile, Workspace},
    xapi::ConnectionSettings,
};
#[derive(Deserialize)]
struct Credentials {
    settings: ConnectionSettings,
    password: String,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let mut input: Credentials = serde_json::from_str(&line)?;
    drop(line);
    input.settings.read_only = true;
    let profile: Profile = serde_json::from_value(
        serde_json::json!({"id":"production-offline","settings":{"host":"10.100.1.11","username":"root","label":"Production","read_only":true}}),
    )?;
    let workspace = Workspace::with_profiles(vec![profile]);
    let id = workspace
        .connect(input.settings.clone(), &input.password, None)
        .await?;
    let view = workspace.view().await;
    assert_eq!(view.connections.len(), 2);
    let vm_uuid = view
        .connections
        .iter()
        .find_map(|c| match c {
            ConnectionView::Online { inventory, .. } => {
                inventory.vms.first().map(|vm| vm.uuid.clone())
            }
            _ => None,
        })
        .ok_or("No guest VM for the read-only boundary test")?;
    let error = workspace
        .start_action(id.clone(), vm_uuid, VmAction::Start)
        .await
        .err()
        .ok_or("Read-only action was accepted")?;
    assert!(error.contains("read-only"));
    assert!(workspace.view().await.tasks.is_empty());
    assert!(
        workspace
            .connect(input.settings, &input.password, None)
            .await
            .is_err()
    );
    drop(input.password);
    workspace.refresh(&id).await?;
    workspace.disconnect(&id).await?;
    assert!(
        workspace
            .view()
            .await
            .connections
            .iter()
            .all(|c| matches!(c, ConnectionView::Offline { .. }))
    );
    println!("PASS: connected host + offline production profile coexist");
    println!("PASS: mutation denied in read-only mode; no task created");
    println!("PASS: duplicate pool rejected without replacing the active connection");
    println!("PASS: refresh and logout of one connection preserve the other profile");
    println!("No login or operation was sent to production.");
    Ok(())
}
