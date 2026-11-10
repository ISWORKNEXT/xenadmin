use std::error::Error;

use reqwest::Url;
use xen_core::transport::{HostTransport, TlsOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let origin = args
        .next()
        .ok_or("Usage: tls_probe https://HOST [--insecure]")?;
    let bypass = match args.next().as_deref() {
        None => false,
        Some("--insecure") => true,
        Some(_) => return Err("Expected --insecure".into()),
    };
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let client = HostTransport::new(
        Url::parse(&origin)?,
        TlsOptions {
            bypass_certificate_verification: bypass,
        },
    )?;
    match client.probe().await {
        Ok(response) => {
            println!("bypass={bypass} HTTP {}", response.status().as_u16());
            if !response.status().is_success() {
                return Err("Host did not return a successful HTTP response".into());
            }
        }
        Err(error) => {
            eprintln!("bypass={bypass} connection failed");
            let mut cause = error.source();
            while let Some(detail) = cause {
                eprintln!("  {detail}");
                cause = detail.source();
            }
            return Err("TLS probe failed".into());
        }
    }
    Ok(())
}
