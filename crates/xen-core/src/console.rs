//! VNC console transport. The console endpoint is reached through the same
//! HTTPS origin as XenAPI, using an HTTP CONNECT tunnel authenticated with the
//! existing session cookie. The returned stream starts at the RFB handshake.
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use url::Url;

use crate::xapi::{ApiError, Connection};

pub struct ConsoleTunnel {
    pub console_uuid: String,
    pub location: String,
    pub protocol: String,
    pub stream: TlsStream<TcpStream>,
}

impl Connection {
    pub async fn console_tunnel(&self, vm_uuid: &str) -> Result<ConsoleTunnel, ApiError> {
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let consoles: Vec<String> = self
            .request("VM.get_consoles", json!([self.session, reference]))
            .await?;
        let mut console = None;
        for console_ref in consoles {
            let record: Value = self
                .request("console.get_record", json!([self.session, console_ref]))
                .await?;
            let protocol = record["protocol"].as_str().unwrap_or_default();
            if protocol == "rfb" {
                console = Some(record);
                break;
            }
        }
        let console = console
            .ok_or_else(|| ApiError::Xen("This VM does not expose a VNC console.".into()))?;
        let location = console["location"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(ApiError::Protocol)?;
        let console_uuid = console["uuid"].as_str().unwrap_or_default().to_owned();
        let protocol = console["protocol"].as_str().unwrap_or_default().to_owned();
        let url = Url::parse(location).map_err(|_| ApiError::Protocol)?;
        if url.scheme() != "https" {
            return Err(ApiError::Protocol);
        }
        let host = url.host_str().ok_or(ApiError::Protocol)?.to_owned();
        let port = url.port_or_known_default().ok_or(ApiError::Protocol)?;
        let address = format!("{host}:{port}");
        let tcp = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(&address))
            .await
            .map_err(|_| ApiError::Connection)?
            .map_err(|_| ApiError::Connection)?;

        let config = self.tls_config()?;
        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(host.clone()).map_err(|_| ApiError::Protocol)?;
        let mut stream =
            tokio::time::timeout(Duration::from_secs(10), connector.connect(server_name, tcp))
                .await
                .map_err(|_| ApiError::Connection)?
                .map_err(|_| ApiError::Connection)?;

        let mut target = url.path().to_owned();
        if let Some(query) = url.query() {
            target.push('?');
            target.push_str(query);
        }
        let request = format!(
            "CONNECT {target} HTTP/1.0\r\nHost: {host}\r\nCookie: session_id={}\r\n\r\n",
            self.session
        );
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|_| ApiError::Connection)?;

        let mut headers = Vec::new();
        let mut byte = [0u8; 1];
        while headers.len() < 16 * 1024 {
            stream
                .read_exact(&mut byte)
                .await
                .map_err(|_| ApiError::Connection)?;
            headers.push(byte[0]);
            if headers.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let headers = String::from_utf8_lossy(&headers);
        let status = headers.lines().next().unwrap_or_default();
        if !status.starts_with("HTTP/1.") || !status.contains(" 200 ") {
            return Err(ApiError::Xen("The host refused the console tunnel.".into()));
        }

        Ok(ConsoleTunnel {
            console_uuid,
            location: location.to_owned(),
            protocol,
            stream,
        })
    }

    fn tls_config(&self) -> Result<ClientConfig, ApiError> {
        if self.settings.tls.bypass_certificate_verification {
            let provider = rustls::crypto::aws_lc_rs::default_provider();
            return Ok(ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(Arc::new(
                    provider,
                ))))
                .with_no_client_auth());
        }
        let mut roots = RootCertStore::empty();
        let loaded = rustls_native_certs::load_native_certs();
        for certificate in loaded.certs {
            let _ = roots.add(certificate);
        }
        Ok(ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth())
    }
}

#[derive(Debug)]
struct AcceptAnyCertificate(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
