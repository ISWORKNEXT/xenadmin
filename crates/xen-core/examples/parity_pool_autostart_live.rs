//! Opt-in pool autostart checks; the pre-existing exact-key journal remains for UI qualification/restoration.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
};
use xen_core::{
    pool_options::PoolOptionsEdit,
    xapi::{Connection, ConnectionSettings},
};
type Error = Box<dyn std::error::Error>;
#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    owner: Value,
}
fn read(owner: &Value) -> Result<Value, Error> {
    let mut input = owner.clone();
    input["mode"] = "read".into();
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/audit_pool_autostart.py");
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
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only authorized writable .170 is permitted".into());
    }
    let before = read(&input.owner)?;
    let uuid = before["state"]["pool_uuid"]
        .as_str()
        .ok_or("Missing journal pool UUID")?;
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let original = c.pool_options(uuid).await?;
    c.update_pool_options(&PoolOptionsEdit {
        original: original.clone(),
        edited: original.clone(),
    })
    .await?;
    if read(&input.owner)?["state"] != before["state"] {
        return Err("No-op normalized pool state".into());
    }
    let mut edited = original.clone();
    edited.vm_autostart = !original.vm_autostart;
    let mut invalid = edited.clone();
    invalid.uuid = uuid::Uuid::now_v7().to_string();
    if c.update_pool_options(&PoolOptionsEdit {
        original: original.clone(),
        edited: invalid,
    })
    .await
    .is_ok()
        || read(&input.owner)?["state"] != before["state"]
    {
        return Err("Mismatched target changed pool state".into());
    }
    if original.igmp_snooping_enabled.is_some() {
        let mut invalid = edited.clone();
        invalid.igmp_snooping_enabled = None;
        if c.update_pool_options(&PoolOptionsEdit {
            original: original.clone(),
            edited: invalid,
        })
        .await
        .is_ok()
            || read(&input.owner)?["state"] != before["state"]
        {
            return Err("Invalid scalar edit did not precede autostart write".into());
        }
    }
    let mut ro = input.settings;
    ro.read_only = true;
    let readonly = Connection::login(ro, &input.password).await?;
    if readonly
        .update_pool_options(&PoolOptionsEdit {
            original: original.clone(),
            edited: edited.clone(),
        })
        .await
        .is_ok()
        || read(&input.owner)?["state"] != before["state"]
    {
        return Err("Read-only autostart update changed state".into());
    }
    readonly.logout().await?;
    let saved = c
        .update_pool_options(&PoolOptionsEdit {
            original: original.clone(),
            edited: edited.clone(),
        })
        .await?;
    c.update_pool_options(&PoolOptionsEdit {
        original: original.clone(),
        edited: edited.clone(),
    })
    .await?;
    let after = read(&input.owner)?;
    let mut expected = before["state"].clone();
    expected["auto_poweron"] = json!({"present":true,"value":edited.vm_autostart.to_string()});
    if after["state"] != expected || saved != edited {
        return Err("Autostart changed unrelated pool/host/VM state".into());
    }
    let inventory = c.inventory().await?;
    if inventory.hosts.len() != 1 {
        return Err("Expected single dev host".into());
    }
    let host = c.host_details(&inventory.hosts[0].uuid).await?;
    let label = host
        .general
        .iter()
        .flat_map(|group| &group.fields)
        .find(|field| field.label == "VM autostart")
        .ok_or("Host autostart label missing")?;
    if label.value != if edited.vm_autostart { "Yes" } else { "No" } {
        return Err("Derived host autostart state differs".into());
    }
    println!(
        "{}",
        json!({"check":"pool-autostart-core","status":"VERIFIED","pool_uuid":uuid,"new_value":edited.vm_autostart,"noop_exact":true,"mismatched_target_rejected":true,"invalid_scalar_rejected":original.igmp_snooping_enabled.is_some(),"readonly_rejected":true,"idempotent_retry":true,"all_unrelated_state_preserved":true,"host_label":label.value,"state":after["state"]})
    );
    c.update_pool_options(&PoolOptionsEdit {
        original: saved,
        edited: original.clone(),
    })
    .await?;
    let current = read(&input.owner)?;
    let mut expected = before["state"].clone();
    expected["auto_poweron"] = json!({"present":true,"value":original.vm_autostart.to_string()});
    if current["state"] != expected {
        return Err("Semantic flag restore changed other state".into());
    }
    println!(
        "{}",
        json!({"phase":"ready-for-native","original_boolean_restored":true,"exact_key_presence_restore_pending_journal":true,"pool_uuid":uuid,"state":current["state"]})
    );
    c.logout().await?;
    Ok(())
}
