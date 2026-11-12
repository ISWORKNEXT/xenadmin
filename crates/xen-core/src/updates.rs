//! Read-only GitHub release checks.
pub async fn latest_release(current: &str) -> Result<(String, String), String> {
    let response = reqwest::Client::builder()
        .user_agent("Xen-Desktop")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .get("https://api.github.com/repos/ISWORKNEXT/xenadmin/releases/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or("Missing release version")?;
    let url = value["html_url"]
        .as_str()
        .filter(|s| s.starts_with("https://github.com/ISWORKNEXT/xenadmin/releases/"))
        .ok_or("Invalid release URL")?;
    let version = |s: &str| -> Option<Vec<u64>> {
        s.trim_start_matches('v')
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    };
    let message = match (version(tag), version(current)) {
        (Some(latest), Some(installed)) if latest > installed => {
            format!("Update available: {tag} (installed {current}).")
        }
        (Some(_), Some(_)) => {
            format!("You are up to date (installed {current}, latest release {tag}).")
        }
        _ => format!("Latest release: {tag}; installed: {current}."),
    };
    Ok((message, url.into()))
}
