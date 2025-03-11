use rustls::{crypto::CryptoProvider, pki_types::CertificateDer};

pub(crate) struct Provider;

// Default Crypto Provider `ring`. NOTE : This will be the default even if both `ring` and `aws-lc-rs` feature flags are enabled.
#[cfg(feature = "ring")]
impl Provider {
    pub fn default() -> CryptoProvider {
        rustls::crypto::ring::default_provider()
    }

    pub fn sha256(cert: &CertificateDer<'_>) -> ring::digest::Digest {
        ring::digest::digest(&ring::digest::SHA256, cert)
    }
}

// Crypto Provider if and only if `aws-lc-rs` feature flag is enabled
#[cfg(all(feature = "aws-lc-rs", not(feature = "ring")))]
impl Provider {
    pub fn default() -> CryptoProvider {
        rustls::crypto::aws_lc_rs::default_provider()
    }

    pub fn sha256(cert: &CertificateDer<'_>) -> aws_lc_rs::digest::Digest {
        aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, cert)
    }
}
