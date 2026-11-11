//! Opt-in real pool-option round trips on the authorized development host.
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    process::{Command, Stdio},
};
use xen_core::{
    pool_options::{PoolOptions, PoolOptionsEdit},
    properties::{ObjectKind, ObjectTarget},
    xapi::{Connection, ConnectionSettings},
};
type Error = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
}

fn raw(uuid: &str) -> Result<PoolOptions, Error> {
    let script=r#"
import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','pool-option-independent-readback')
try:
 v=s.xenapi.pool.get_record(s.xenapi.pool.get_by_uuid(UUID))
 out={k:v.get(k) for k in ['uuid','migration_compression','igmp_snooping_enabled','live_patching_disabled']};out['vm_autostart']=v['other_config'].get('auto_poweron')=='true';print(json.dumps(out))
finally:s.xenapi.session.logout()
"#.replace("UUID",&serde_json::to_string(uuid)?);
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "root@192.168.8.170", "python3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing SSH input")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err("Independent pool readback failed".into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn exercise(
    c: &Connection,
    other: &Connection,
    original: &PoolOptions,
    input: &Input,
) -> Result<(), Error> {
    let metadata_target = ObjectTarget {
        kind: ObjectKind::Pool,
        uuid: original.uuid.clone(),
    };
    let metadata = c.object_properties(&metadata_target).await?;
    for index in 0..3 {
        let mut edited = original.clone();
        let (label, value) = match index {
            0 => ("migration_compression", &mut edited.migration_compression),
            1 => ("igmp_snooping_enabled", &mut edited.igmp_snooping_enabled),
            _ => ("live_patching_disabled", &mut edited.live_patching_disabled),
        };
        let Some(current) = *value else {
            println!(
                "{}",
                json!({"setting":label,"status":"NOT VERIFIED","reason":"Field unavailable"})
            );
            continue;
        };
        *value = Some(!current);
        let saved = c
            .update_pool_options(&PoolOptionsEdit {
                original: original.clone(),
                edited: edited.clone(),
            })
            .await?;
        if saved != edited || raw(&original.uuid)? != edited {
            return Err(format!("{label} did not match independent readback").into());
        }
        c.update_pool_options(&PoolOptionsEdit {
            original: saved,
            edited: original.clone(),
        })
        .await?;
        if raw(&original.uuid)? != *original {
            return Err(format!("{label} was not restored").into());
        }
        println!(
            "{}",
            json!({"check":"pool-option-roundtrip","setting":label,"original":current,"temporary":!current,"restored":true,"other_options_preserved":true,"status":"VERIFIED"})
        );
    }
    if let (Some(compression), Some(igmp)) = (
        original.migration_compression,
        original.igmp_snooping_enabled,
    ) {
        let mut invalid = original.clone();
        invalid.migration_compression = Some(!compression);
        invalid.igmp_snooping_enabled = None;
        if c.update_pool_options(&PoolOptionsEdit {
            original: original.clone(),
            edited: invalid,
        })
        .await
        .is_ok()
            || raw(&original.uuid)? != *original
        {
            return Err("Validation did not precede all pool-option writes".into());
        }
        println!(
            "{}",
            json!({"check":"invalid-pool-options-rejected-before-writes","status":"VERIFIED"})
        );
        let mut external = original.clone();
        external.igmp_snooping_enabled = Some(!igmp);
        other
            .update_pool_options(&PoolOptionsEdit {
                original: original.clone(),
                edited: external.clone(),
            })
            .await?;
        let mut requested = original.clone();
        requested.migration_compression = Some(!compression);
        let edit = PoolOptionsEdit {
            original: original.clone(),
            edited: requested,
        };
        let saved = c.update_pool_options(&edit).await?;
        if saved.igmp_snooping_enabled != external.igmp_snooping_enabled
            || saved.migration_compression != Some(!compression)
        {
            return Err("Stale pool form overwrote an unrelated external edit".into());
        }
        if c.update_pool_options(&edit).await? != saved || raw(&original.uuid)? != saved {
            return Err("Retry did not preserve the external edit".into());
        }
        c.update_pool_options(&PoolOptionsEdit {
            original: saved,
            edited: original.clone(),
        })
        .await?;
        println!(
            "{}",
            json!({"check":"pool-options-preserve-external-unchanged-field-and-retry","status":"VERIFIED"})
        );
        let mut settings = input.settings.clone();
        settings.read_only = true;
        let readonly = Connection::login(settings, &input.password).await?;
        let rejected = readonly.update_pool_options(&edit).await.is_err();
        readonly.logout().await?;
        if !rejected || raw(&original.uuid)? != *original {
            return Err("Read-only pool-option write changed the host".into());
        }
        println!(
            "{}",
            json!({"check":"read-only-pool-options-rejected","status":"VERIFIED"})
        );
    }
    let after = c.object_properties(&metadata_target).await?;
    if metadata.name != after.name
        || metadata.description != after.description
        || metadata.tags != after.tags
    {
        return Err("Pool identity metadata changed during option edits".into());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only the authorized writable development host is permitted".into());
    }
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let other = Connection::login(input.settings.clone(), &input.password).await?;
    let pool = c.inventory().await?.pool_id;
    let original = c.pool_options(&pool).await?;
    if raw(&pool)? != original {
        return Err("Initial pool readback differs".into());
    }
    let journal_path = std::env::temp_dir().join(format!(
        "zz-parity-pool-options-{}.json",
        uuid::Uuid::now_v7()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut journal = options.open(&journal_path)?;
    journal.write_all(serde_json::to_string(&original)?.as_bytes())?;
    journal.sync_all()?;
    println!(
        "{}",
        json!({"phase":"baseline","options":original,"rollback_journal":journal_path})
    );
    let result = exercise(&c, &other, &original, &input).await;
    let restored = async {
        let current = c.pool_options(&pool).await?;
        c.update_pool_options(&PoolOptionsEdit {
            original: current,
            edited: original.clone(),
        })
        .await?;
        if raw(&pool)? != original {
            return Err("Final pool option restoration differs".into());
        }
        Ok::<(), Error>(())
    }
    .await;
    other.logout().await?;
    c.logout().await?;
    if let Err(error) = restored {
        return Err(format!(
            "Pool-option restoration failed: {error}; journal retained at {}",
            journal_path.display()
        )
        .into());
    }
    drop(journal);
    std::fs::remove_file(journal_path)?;
    println!(
        "{}",
        json!({"phase":"restored","options":original,"rollback_journal_removed":true,"result":result.as_ref().map(|_|"pass").map_err(|e|e.to_string())})
    );
    result
}
