use std::time::Duration;

use reqwest::{Client, Response, Url, redirect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Settings belong to one saved host connection. Certificate bypass is opt-in.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsOptions {
    pub bypass_certificate_verification: bool,
}

/// A client bound to one HTTPS origin. Its TLS policy cannot affect other hosts.
pub struct HostTransport {
    client: Client,
    origin: Url,
    proxy: Option<String>,
}

impl HostTransport {
    pub fn new(origin: Url, tls: TlsOptions, proxy: Option<&str>) -> Result<Self, TransportError> {
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(TransportError::InvalidOrigin);
        }

        let proxy = proxy
            .map(str::trim)
            .filter(|proxy| !proxy.is_empty())
            .map(str::to_owned);
        let mut builder = Client::builder()
            .tls_danger_accept_invalid_certs(tls.bypass_certificate_verification)
            .redirect(redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45));
        if let Some(proxy) = proxy.as_deref() {
            builder = builder
                .proxy(reqwest::Proxy::all(proxy).map_err(|_| TransportError::InvalidProxy)?);
        }
        let client = builder.build().map_err(TransportError::Http)?;

        Ok(Self {
            client,
            origin,
            proxy,
        })
    }

    pub async fn probe(&self) -> Result<Response, reqwest::Error> {
        self.client.get(self.origin.clone()).send().await
    }

    /// Redirects are returned to the caller, never followed with session data.
    pub async fn post_json_rpc(&self, request: &Value) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/jsonrpc");
        self.client.post(endpoint).json(request).send().await
    }

    pub(crate) async fn host_backup(&self, session: &str) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/host_backup");
        self.client
            .get(endpoint)
            .query(&[("session_id", session)])
            .timeout(Duration::from_secs(600))
            .send()
            .await
    }
    pub(crate) async fn host_restore(
        &self,
        session: &str,
        file: tokio::fs::File,
        length: u64,
    ) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/host_restore");
        self.client
            .put(endpoint)
            .query(&[("session_id", session)])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::from(file))
            .timeout(Duration::from_secs(600))
            .send()
            .await
    }

    pub(crate) async fn export_xva(
        &self,
        session: &str,
        uuid: &str,
        task: &str,
    ) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/export");
        self.client
            .get(endpoint)
            .query(&[("session_id", session), ("uuid", uuid), ("task_id", task)])
            .timeout(Duration::from_secs(24 * 60 * 60))
            .send()
            .await
    }

    pub(crate) async fn import_xva(
        &self,
        session: &str,
        sr: &str,
        task: &str,
        file: tokio::fs::File,
        length: u64,
    ) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/import");
        self.client
            .put(endpoint)
            .query(&[
                ("session_id", session),
                ("sr_id", sr),
                ("task_id", task),
                ("restore", "false"),
                ("force", "false"),
            ])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::from(file))
            .timeout(Duration::from_secs(24 * 60 * 60))
            .send()
            .await
    }

    pub(crate) async fn raw_vdi(
        &self,
        session: &str,
        vdi: &str,
        task: &str,
        upload: Option<(tokio::fs::File, u64, &str)>,
    ) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path(if upload.is_some() {
            "/import_raw_vdi"
        } else {
            "/export_raw_vdi"
        });
        let (method, format) = if let Some((_, _, format)) = &upload {
            (reqwest::Method::PUT, *format)
        } else {
            (reqwest::Method::GET, "vhd")
        };
        let mut request = self
            .client
            .request(method, endpoint)
            .query(&[
                ("session_id", session),
                ("vdi", vdi),
                ("task_id", task),
                ("format", format),
            ])
            .timeout(Duration::from_secs(24 * 60 * 60));
        if let Some((file, length, _)) = upload {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .header(reqwest::header::CONTENT_LENGTH, length)
                .body(reqwest::Body::from(file));
        }
        request.send().await
    }

    /// Only called after resolving a member host from the authenticated pool.
    pub(crate) fn for_member(
        &self,
        address: &str,
        tls: TlsOptions,
    ) -> Result<Self, TransportError> {
        let mut origin = self.origin.clone();
        origin
            .set_host(Some(address))
            .map_err(|_| TransportError::InvalidOrigin)?;
        Self::new(origin, tls, self.proxy.as_deref())
    }

    pub(crate) async fn rrd_updates(
        &self,
        session: &str,
        start: u64,
        interval: u64,
    ) -> Result<Response, reqwest::Error> {
        let mut endpoint = self.origin.clone();
        endpoint.set_path("/rrd_updates");
        // Authenticated URLs stay in Rust and are never returned or logged.
        self.client
            .get(endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .query(&[
                ("session_id", session.to_owned()),
                ("start", start.to_string()),
                ("interval", interval.to_string()),
                ("cf", "AVERAGE".into()),
                ("host", "true".into()),
                ("json", "true".into()),
            ])
            .send()
            .await
    }
}

#[derive(Debug)]
pub enum TransportError {
    InvalidOrigin,
    InvalidProxy,
    Http(reqwest::Error),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrigin => {
                f.write_str("Expected an HTTPS origin without credentials or a path")
            }
            Self::InvalidProxy => f.write_str("Expected a valid proxy URL"),
            Self::Http(_) => f.write_str("Could not create host HTTP client"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::InvalidOrigin | Self::InvalidProxy => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_connection_settings_keep_certificate_verification() {
        let tls: TlsOptions = serde_json::from_str("{}").unwrap();
        assert!(!tls.bypass_certificate_verification);
    }

    #[test]
    fn bypass_requires_explicit_setting() {
        let tls: TlsOptions =
            serde_json::from_str(r#"{"bypass_certificate_verification":true}"#).unwrap();
        assert!(tls.bypass_certificate_verification);
        assert!(serde_json::from_str::<TlsOptions>(r#"{"bypass_certificates":true}"#).is_err());
    }

    #[test]
    fn validates_and_builds_proxy_clients() {
        let origin = Url::parse("https://localhost/").unwrap();
        assert!(
            HostTransport::new(
                origin.clone(),
                TlsOptions::default(),
                Some("http://127.0.0.1:3128")
            )
            .is_ok()
        );
        assert!(matches!(
            HostTransport::new(origin, TlsOptions::default(), Some("not a proxy")),
            Err(TransportError::InvalidProxy)
        ));
    }

    #[test]
    fn rejects_credentials_plaintext_and_non_origin_urls() {
        for value in [
            "http://localhost/",
            "https://user:password@localhost/",
            "https://localhost/jsonrpc",
            "https://localhost/?token=secret",
            "https://localhost/#fragment",
        ] {
            assert!(matches!(
                HostTransport::new(Url::parse(value).unwrap(), TlsOptions::default(), None),
                Err(TransportError::InvalidOrigin)
            ));
        }
    }
}
