use std::io::{self, BufRead};

use serde::Deserialize;
use xen_core::xapi::{Connection, ConnectionSettings};

#[derive(Deserialize)]
struct Credentials {
    settings: ConnectionSettings,
    password: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let credentials: Credentials = serde_json::from_str(&line)?;
    drop(line);
    let connection = Connection::login(credentials.settings, &credentials.password).await?;
    drop(credentials.password);
    let result = connection.inventory().await;
    let logout = connection.logout().await;
    let inventory = result?;
    println!(
        "Hosts: {}, guest VMs: {}, SRs: {}, networks: {}",
        inventory.hosts.len(),
        inventory.vms.len(),
        inventory.storage.len(),
        inventory.networks.len()
    );
    for host in &inventory.hosts {
        println!(
            "{}: {} {}, {} CPUs, {} bytes memory",
            host.name, host.brand, host.version, host.cpu_count, host.memory_total
        );
    }
    logout?;
    println!("Logout: OK");
    Ok(())
}
