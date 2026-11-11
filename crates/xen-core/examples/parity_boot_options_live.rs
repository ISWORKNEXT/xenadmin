//! Opt-in boot configuration qualification on one exact owned diskless .170 VM.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
};
use xen_core::{
    operations::VmResourceUpdate,
    xapi::{Connection, ConnectionSettings},
};
type Error = Box<dyn std::error::Error>;
#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    owner: Value,
}
fn probe(owner: &Value, mode: &str) -> Result<Value, Error> {
    let mut input = owner.clone();
    input["mode"] = mode.into();
    let helper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/audit_boot_object.py");
    let mut child = Command::new("python3")
        .arg(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing helper stdin")?
        .write_all(input.to_string().as_bytes())?;
    let result = child.wait_with_output()?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only authorized writable .170 is permitted".into());
    }
    let uuid = input.owner["vm_uuid"]
        .as_str()
        .ok_or("Missing owner UUID")?;
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let before = probe(&input.owner, "read")?;
    let d = c.vm_details(uuid).await?;
    if !d.hvm_boot_policy.is_empty() {
        return Err("Expected PV source".into());
    }
    let mut update = VmResourceUpdate {
        vcpus_max: d.vcpus_max,
        vcpus_at_startup: d.vcpus_at_startup,
        memory_static_max_mib: d.memory_static_max / (1024 * 1024),
        memory_dynamic_min_mib: d.memory_dynamic_min / (1024 * 1024),
        memory_dynamic_max_mib: d.memory_dynamic_max / (1024 * 1024),
        boot_order: None,
        firmware: None,
        secure_boot: None,
        boot_policy: None,
        after_crash: None,
        cpu_weight: None,
        cores_per_socket: None,
        pv_args: None,
        auto_poweron: None,
    };
    for invalid in ["nul\0args".to_owned(), "x".repeat(32_768)] {
        update.pv_args = Some(invalid);
        update.auto_poweron = Some(true);
        if c.update_vm_resources(uuid, &update).await.is_ok()
            || probe(&input.owner, "read")?["record"] != before["record"]
        {
            return Err("Invalid PV args wrote VM fields".into());
        }
    }
    let hvm = probe(&input.owner, "hvm")?;
    update.pv_args = Some("console=hvc0 changed=1".into());
    let error = c
        .update_vm_resources(uuid, &update)
        .await
        .err()
        .ok_or("HVM accepted PV args")?
        .to_string();
    if !error.contains("only available for PV guests")
        || probe(&input.owner, "read")?["record"] != hvm["record"]
    {
        return Err("HVM PV guard did not reject before writes".into());
    }
    probe(&input.owner, "pv")?;
    let mut ro = input.settings;
    ro.read_only = true;
    let readonly = Connection::login(ro, &input.password).await?;
    if readonly.update_vm_resources(uuid, &update).await.is_ok()
        || probe(&input.owner, "read")?["record"] != before["record"]
    {
        return Err("Read-only boot update changed state".into());
    }
    readonly.logout().await?;
    update.pv_args = Some("console=hvc0 root=/dev/xvda1  loglevel=7".into());
    update.auto_poweron = Some(true);
    c.update_vm_resources(uuid, &update).await?;
    let after = probe(&input.owner, "read")?;
    let mut expected = before["record"].clone();
    expected["PV_args"] = json!(update.pv_args);
    expected["other_config"]["auto_poweron"] = "true".into();
    if after["record"] != expected {
        return Err("Boot write changed unrelated VM fields".into());
    }
    let detail = c.vm_details(uuid).await?;
    if !detail.auto_poweron || Some(detail.pv_args) != update.pv_args {
        return Err("VmDetails boot readback differs".into());
    }
    println!(
        "{}",
        json!({"check":"boot-options-core","status":"VERIFIED","nul_rejected":true,"overlong_rejected":true,"hvm_guard_no_writes":true,"readonly_no_writes":true,"only_requested_fields_changed":true,"host_pool_maps_unchanged":after["global_state_unchanged"],"state":after["record"]})
    );
    update.pv_args = Some(d.pv_args);
    update.auto_poweron = Some(false);
    c.update_vm_resources(uuid, &update).await?;
    let legacy = probe(&input.owner, "legacy-auto")?;
    update.pv_args = None;
    update.auto_poweron = None;
    c.update_vm_resources(uuid, &update).await?;
    if probe(&input.owner, "read")?["record"] != legacy["record"] {
        return Err("Omitted boot fields normalized legacy state".into());
    }
    println!(
        "{}",
        json!({"check":"omitted-boot-fields-preserve-legacy","status":"VERIFIED","state":legacy["record"],"host_pool_maps_unchanged":true})
    );
    c.logout().await?;
    Ok(())
}
