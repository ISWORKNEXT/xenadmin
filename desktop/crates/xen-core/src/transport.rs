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
}

impl HostTransport {
    pub fn new(origin: Url, tls: TlsOptions) -> Result<Self, TransportError> {
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

        let client = Client::builder()
            .tls_danger_accept_invalid_certs(tls.bypass_certificate_verification)
            .redirect(redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(TransportError::Http)?;

        Ok(Self { client, origin })
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
        Self::new(origin, tls)
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
    Http(reqwest::Error),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrigin => {
                f.write_str("Expected an HTTPS origin without credentials or a path")
            }
            Self::Http(_) => f.write_str("Could not create host HTTP client"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::InvalidOrigin => None,
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
    fn rejects_credentials_plaintext_and_non_origin_urls() {
        for value in [
            "http://localhost/",
            "https://user:password@localhost/",
            "https://localhost/jsonrpc",
            "https://localhost/?token=secret",
            "https://localhost/#fragment",
        ] {
            assert!(matches!(
                HostTransport::new(Url::parse(value).unwrap(), TlsOptions::default()),
                Err(TransportError::InvalidOrigin)
            ));
        }
    }
}
