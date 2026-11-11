//! Opt-in real Windows-template creation guard; invalid SR prevents any clone even if the guard regresses.
use serde::Deserialize;
use serde_json::json;
use std::io;
use xen_core::{
    provisioning::{CreateVmRequest, VmCreationSource},
    xapi::{Connection, ConnectionSettings},
};

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only authorized writable .170 is permitted".into());
    }
    let c = Connection::login(input.settings, &input.password).await?;
    let inventory = c.inventory().await?;
    let source = inventory
        .templates
        .iter()
        .find(|vm| vm.name == "Windows 10 (64-bit)")
        .ok_or("Windows template missing")?;
    let before = c.vm_details(&source.uuid).await?;
    if before.supports_vcpu_hotplug {
        return Err("Windows template unexpectedly advertises separate CPU counts".into());
    }
    let error = c
        .create_configured_vm(&CreateVmRequest {
            source: VmCreationSource::Template(source.uuid.clone()),
            name: "zz-parity-cpu-guard-must-not-exist".into(),
            description: before.description.clone(),
            vcpus_max: 4,
            vcpus_at_startup: 2,
            cores_per_socket: Some(1),
            memory_mib: 1024,
            sr_uuid: String::new(),
            new_disk_gib: None,
            network_uuid: None,
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await
        .err()
        .ok_or("Windows unequal CPU request was admitted")?
        .to_string();
    if !error.contains("does not support separate initial and maximum vCPU counts") {
        return Err(format!("Did not exercise the intended preclone guard: {error}").into());
    }
    let after = c.vm_details(&source.uuid).await?;
    if serde_json::to_value(&before)? != serde_json::to_value(&after)?
        || c.inventory()
            .await?
            .vms
            .iter()
            .any(|vm| vm.name == "zz-parity-cpu-guard-must-not-exist")
    {
        return Err("Guard changed source or created an output".into());
    }
    println!(
        "{}",
        json!({"status":"VERIFIED","check":"windows-template-separate-cpu-guard","source_uuid":source.uuid,"source_unchanged":true,"error":error,"invalid_sr_prevents_clone_if_guard_regresses":true})
    );
    c.logout().await?;
    Ok(())
}
