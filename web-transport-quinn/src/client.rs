use std::sync::Arc;

use tokio::net::lookup_host;
use url::Url;

use quinn::{crypto::rustls::QuicClientConfig, rustls};
use rustls::{client::danger::ServerCertVerifier, pki_types::CertificateDer};

use crate::{ClientError, Session, ALPN};

// Copies the Web options, hiding the actual implementation.
/// Allows specifying a class of congestion control algorithm.
pub enum CongestionControl {
    Default,
    Throughput,
    LowLatency,
}

/// Construct a WebTransport [Client] using sane defaults.
///
/// This is optional; advanced users may use [Client::new] directly.
#[derive(Default)]
pub struct ClientBuilder {
    congestion_controller:
        Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
}

impl ClientBuilder {
    /// Create a Client builder, which can be used to establish multiple [Session]s.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable the specified congestion controller.
    pub fn with_congestion_control(mut self, algorithm: CongestionControl) -> Self {
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

    /// Accept any certificate from the server if it uses a known root CA.
    pub fn with_system_roots(self) -> Result<Client, ClientError> {
        let mut roots = rustls::RootCertStore::empty();

        let native = rustls_native_certs::load_native_certs();

        // Log any errors that occurred while loading the native root certificates.
        for err in native.errors {
            log::warn!("failed to load root cert: {:?}", err);
        }

        // Add the platform's native root certificates.
        for cert in native.certs {
            roots.add(cert)?;
        }

        let crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();

        self.build(crypto)
    }

    /// Supply certificates for accepted servers instead of using root CAs.
    pub fn with_server_certificates(
        self,
        certs: Vec<CertificateDer>,
    ) -> Result<Client, ClientError> {
        let hashes = certs.iter().map(|cert| {
            aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, cert)
                .as_ref()
                .to_vec()
        });

        self.with_server_certificate_hashes(hashes.collect())
    }

    /// Supply sha256 hashes for accepted certificates instead of using root CAs.
    pub fn with_server_certificate_hashes(
        self,
        hashes: Vec<Vec<u8>>,
    ) -> Result<Client, ClientError> {
        // Use a custom fingerprint verifier.
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let fingerprints = Arc::new(ServerFingerprints {
            provider: provider.clone(),
            fingerprints: hashes,
        });

        // Configure the crypto client.
        let crypto = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(fingerprints.clone())
            .with_no_client_auth();

        self.build(crypto)
    }

    fn build(self, mut crypto: rustls::ClientConfig) -> Result<Client, ClientError> {
        crypto.alpn_protocols = vec![ALPN.to_vec()];

        let client_config = QuicClientConfig::try_from(crypto).unwrap();
        let mut client_config = quinn::ClientConfig::new(Arc::new(client_config));

        let mut transport = quinn::TransportConfig::default();
        if let Some(cc) = &self.congestion_controller {
            transport.congestion_controller_factory(cc.clone());
        }

        client_config.transport_config(transport.into());

        let client = quinn::Endpoint::client("[::]:0".parse().unwrap()).unwrap();
        Ok(Client {
            endpoint: client,
            config: client_config,
        })
    }
}

/// A client for connecting to a WebTransport server.
pub struct Client {
    endpoint: quinn::Endpoint,
    config: quinn::ClientConfig,
}

impl Client {
    /// Manually create a client via a Quinn endpoint and config.
    ///
    /// The ALPN MUST be set to [ALPN].
    pub fn new(endpoint: quinn::Endpoint, config: quinn::ClientConfig) -> Self {
        Self { endpoint, config }
    }

    /// Connect to the server.
    pub async fn connect(&self, url: &Url) -> Result<Session, ClientError> {
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
        let conn = self
            .endpoint
            .connect_with(self.config.clone(), remote, &host)?;
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
