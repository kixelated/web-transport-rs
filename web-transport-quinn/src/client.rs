use std::sync::Arc;

use tokio::net::lookup_host;
use url::Url;

use quinn::{crypto::rustls::QuicClientConfig, rustls};
use rustls::{client::danger::ServerCertVerifier, ClientConfig};

use rustls_platform_verifier::ConfigVerifierExt;

use crate::{ClientError, Session, ALPN};

// Copies the Web options, hiding the actual implementation.
/// Allows specifying a class of congestion control algorithm.
pub enum CongestionControl {
    Default,
    Throughput,
    LowLatency,
}

#[derive(Default)]
pub struct Client {
    congestion_controller:
        Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    fingerprints: Option<Arc<ServerFingerprints>>,
}

impl Client {
    /// Create a [SessionClient] which can be used to build a session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable the specified congestion controller.
    pub fn congestion_control(mut self, algorithm: CongestionControl) -> Self {
        self.congestion_controller = match algorithm {
            CongestionControl::LowLatency => {
                Some(Arc::new(quinn::congestion::BbrConfig::default()))
            }
            // TODO BBR is also higher throughput in theory.
            CongestionControl::Throughput => {
                Some(Arc::new(quinn::congestion::CubicConfig::default()))
            }
            CongestionControl::Default => None,
        };

        self
    }

    /// Supply sha256 hashes for accepted certificates.
    /// If empty, this feature is disabled and root CAs are used.
    pub fn server_certificate_hashes(mut self, hashes: Vec<Vec<u8>>) -> Self {
        if hashes.is_empty() {
            self.fingerprints = None;
            return self;
        }

        // Use a custom fingerprint verifier.
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        self.fingerprints = Some(Arc::new(ServerFingerprints {
            provider,
            fingerprints: hashes,
        }));

        self
    }

    /// Connect to the server.
    pub async fn connect(&self, url: &Url) -> Result<Session, ClientError> {
        // Configure the crypto client.
        let client_crypto = rustls::ClientConfig::builder();
        let mut client_crypto = match &self.fingerprints {
            Some(fingerprints) => client_crypto
                .dangerous()
                .with_custom_certificate_verifier(fingerprints.clone())
                .with_no_client_auth(),
            None => ClientConfig::with_platform_verifier(),
        };
        client_crypto.alpn_protocols = vec![ALPN.to_vec()];

        let client_config = QuicClientConfig::try_from(client_crypto).unwrap();
        let mut client_config = quinn::ClientConfig::new(Arc::new(client_config));

        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(std::time::Duration::from_secs(10).try_into().unwrap()));
        transport.keep_alive_interval(Some(std::time::Duration::from_secs(4))); // TODO make this smarter

        if let Some(cc) = &self.congestion_controller {
            transport.congestion_controller_factory(cc.clone());
        }

        client_config.transport_config(transport.into());

        let client = quinn::Endpoint::client("[::]:0".parse().unwrap()).unwrap();

        // TODO error on username:password in host
        let host = url
            .host()
            .ok_or_else(|| ClientError::InvalidDnsName("".to_string()))?
            .to_string();

        let port = url.port().unwrap_or(443);

        // Look up the DNS entry.
        let mut remotes = match lookup_host((host.clone(), port)).await {
            Ok(remotes) => remotes,
            Err(_) => return Err(ClientError::InvalidDnsName(host)),
        };

        // Return the first entry.
        let remote = match remotes.next() {
            Some(remote) => remote,
            None => return Err(ClientError::InvalidDnsName(host)),
        };

        // Connect to the server using the addr we just resolved.
        let conn = client.connect_with(client_config, remote, &host)?;
        let conn = conn.await?;

        // Connect with the connection we established.
        Session::connect(conn, url).await
    }
}

#[derive(Debug)]
struct ServerFingerprints {
    provider: Arc<rustls::crypto::CryptoProvider>,
    fingerprints: Vec<Vec<u8>>,
}

impl ServerCertVerifier for ServerFingerprints {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let cert_hash = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, end_entity);

        if self
            .fingerprints
            .iter()
            .any(|fingerprint| fingerprint == cert_hash.as_ref())
        {
            return Ok(rustls::client::danger::ServerCertVerified::assertion());
        }

        Err(rustls::Error::InvalidCertificate(
            rustls::CertificateError::UnknownIssuer,
        ))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
