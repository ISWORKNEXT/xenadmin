//! Real development-host qualification. Credentials are read from stdin.
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead, Write};
use xen_core::{
    operations::VmAction,
    properties::{ObjectKind, ObjectPropertiesEdit, ObjectTarget},
    xapi::{Connection, ConnectionSettings},
};

type Error = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    vm_uuid: String,
}

async fn check(
    c: &Connection,
    target: ObjectTarget,
    tag: &str,
    journal: &mut std::fs::File,
) -> Result<(), Error> {
    let original = c.object_properties(&target).await?;
    if (original.name.is_empty() && target.kind != ObjectKind::Pool)
        || original.name.trim() != original.name
        || original.name.chars().count() > 255
        || original.description.chars().count() > 32767
        || original.tags.iter().any(|tag| {
            tag.trim().is_empty() || tag.chars().count() > 255 || tag.chars().any(char::is_control)
        })
    {
        return Err("Existing properties cannot be restored through this editor; no test mutation performed".into());
    }
    writeln!(journal, "{}", serde_json::to_string(&original)?)?;
    journal.sync_data()?;
    let mut changed = original.clone();
    changed.name = format!("{tag}-{:?}", target.kind);
    changed.description = "Temporary real-host property qualification".into();
    changed.tags.push(tag.into());
    let result: Result<(), Error> = async {
        let saved = c
            .update_object_properties(&ObjectPropertiesEdit {
                original: original.clone(),
                edited: changed.clone(),
            })
            .await?;
        if saved.name != changed.name
            || saved.description != changed.description
            || !saved.tags.iter().any(|value| value == tag)
        {
            return Err("Saved metadata does not match server readback".into());
        }
        let mut invalid = changed.clone();
        invalid.name = "This invalid request must not rename the object".into();
        invalid.tags.push("invalid\ntag".into());
        if c.update_object_properties(&ObjectPropertiesEdit {
            original: changed.clone(),
            edited: invalid,
        })
        .await
        .is_ok()
        {
            return Err("Invalid tag accepted".into());
        }
        if c.object_properties(&target).await?.name != changed.name {
            return Err("Validation occurred after a mutation".into());
        }
        let mut concurrent = changed.clone();
        concurrent.name.push_str("-concurrent");
        c.update_object_properties(&ObjectPropertiesEdit {
            original: changed.clone(),
            edited: concurrent.clone(),
        })
        .await?;
        let mut description_only = changed.clone();
        description_only
            .description
            .push_str(" after concurrent rename");
        let preserved = c
            .update_object_properties(&ObjectPropertiesEdit {
                original: changed.clone(),
                edited: description_only,
            })
            .await?;
        if preserved.name != concurrent.name {
            return Err("An unrelated concurrent rename was overwritten".into());
        }
        let mut stale = changed.clone();
        stale.name = "Blocked stale overwrite".into();
        stale.tags.push("blocked-conflict-tag".into());
        if c.update_object_properties(&ObjectPropertiesEdit {
            original: changed.clone(),
            edited: stale,
        })
        .await
        .is_ok()
        {
            return Err("A same-field conflict was accepted".into());
        }
        if c.object_properties(&target)
            .await?
            .tags
            .iter()
            .any(|tag| tag == "blocked-conflict-tag")
        {
            return Err("Conflict checking happened after a mutation".into());
        }
        Ok(())
    }
    .await;
    let current = c.object_properties(&target).await?;
    let restored = c
        .update_object_properties(&ObjectPropertiesEdit {
            original: current,
            edited: original.clone(),
        })
        .await?;
    let mut old_tags = original.tags.clone();
    old_tags.sort();
    let mut new_tags = restored.tags.clone();
    new_tags.sort();
    if restored.name != original.name
        || restored.description != original.description
        || new_tags != old_tags
    {
        return Err("Original properties were not restored".into());
    }
    result?;
    println!(
        "{}",
        json!({"check":"object-properties","kind":target.kind,"uuid":target.uuid,"changed_and_read_back":true,"invalid_request_unchanged":true,"concurrent_unrelated_preserved":true,"same_field_conflict_rejected":true,"restored":true})
    );
    Ok(())
}

async fn exercise(
    c: &Connection,
    input: &Input,
    prefix: &str,
    journal: &mut std::fs::File,
) -> Result<(), Error> {
    let before = c.vm_details(&input.vm_uuid).await?;
    if !before.name.starts_with("zz-parity-advanced-")
        || before.is_template
        || before.is_snapshot
        || before.power_state != "Halted"
    {
        return Err("Use an owned halted qualification VM".into());
    }
    let inventory = c.inventory().await?;
    let host = inventory.hosts.first().ok_or("No development host")?;
    let sr = inventory
        .storage
        .iter()
        .find(|sr| sr.content_type != "iso")
        .ok_or("No data SR")?;
    let disk = before
        .disks
        .iter()
        .find(|disk| !disk.is_cd)
        .ok_or("No owned test disk")?;
    c.create_network(prefix, "Temporary property qualification network", 1500)
        .await?;
    let inventory = c.inventory().await?;
    let network = inventory
        .networks
        .iter()
        .find(|network| network.name == prefix)
        .ok_or("Test network missing")?;
    for (kind, uuid) in [
        (ObjectKind::Vm, before.uuid.clone()),
        (ObjectKind::Disk, disk.vdi_uuid.clone()),
        (ObjectKind::Network, network.uuid.clone()),
        (ObjectKind::Storage, sr.uuid.clone()),
        (ObjectKind::Host, host.uuid.clone()),
        (ObjectKind::Pool, inventory.pool_id.clone()),
    ] {
        check(c, ObjectTarget { kind, uuid }, prefix, journal).await?;
    }
    let name = format!("{prefix}-snapshot");
    let task = c
        .begin_vm_action(&input.vm_uuid, &VmAction::Snapshot { name: name.clone() })
        .await?;
    c.wait_task(&task).await?;
    let inventory = c.inventory().await?;
    let snapshot = inventory
        .snapshots
        .iter()
        .find(|vm| vm.name == name)
        .ok_or("Test snapshot missing")?;
    check(
        c,
        ObjectTarget {
            kind: ObjectKind::Vm,
            uuid: snapshot.uuid.clone(),
        },
        prefix,
        journal,
    )
    .await?;
    let template = c
        .create_from_snapshot(&snapshot.uuid, &format!("{prefix}-template"), true)
        .await?;
    check(
        c,
        ObjectTarget {
            kind: ObjectKind::Vm,
            uuid: template,
        },
        prefix,
        journal,
    )
    .await?;
    let after = c.vm_details(&input.vm_uuid).await?;
    let mut before_disks: Vec<_> = before
        .disks
        .iter()
        .map(|disk| disk.vdi_uuid.clone())
        .collect();
    before_disks.sort();
    let mut after_disks: Vec<_> = after
        .disks
        .iter()
        .map(|disk| disk.vdi_uuid.clone())
        .collect();
    after_disks.sort();
    if before_disks != after_disks
        || before.power_state != after.power_state
        || before.name != after.name
    {
        return Err("Property edits changed the source VM's identity, power or disks".into());
    }
    println!(
        "{}",
        json!({"check":"source-preserved","uuid":input.vm_uuid,"same_disks":true,"same_power":true,"same_name":true})
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only the authorized writable dev host is permitted".into());
    }
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let prefix = format!("zz-parity-properties-{}", uuid::Uuid::now_v7());
    println!("{}", json!({"prefix":prefix}));
    let journal_path = std::env::temp_dir().join(format!("{prefix}-rollback.jsonl"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut journal = options.open(&journal_path)?;
    println!("{}", json!({"rollback_journal":journal_path}));
    let result = exercise(&c, &input, &prefix, &mut journal).await;
    let inventory = c.inventory().await?;
    for vm in inventory
        .templates
        .iter()
        .chain(&inventory.snapshots)
        .filter(|vm| vm.name.starts_with(&prefix))
    {
        c.destroy_vm(&vm.uuid, true, true).await?;
    }
    for network in inventory
        .networks
        .iter()
        .filter(|network| network.name.starts_with(&prefix))
    {
        c.destroy_network(&network.uuid).await?;
    }
    c.logout().await?;
    drop(journal);
    if result.is_ok() {
        std::fs::remove_file(journal_path)?;
    }
    println!(
        "{}",
        json!({"cleanup":"completed","result":result.as_ref().map(|_|"pass").map_err(|error|error.to_string())})
    );
    result
}
