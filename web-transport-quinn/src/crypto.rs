use std::sync::Arc;

use rustls::crypto::hash::{self, HashAlgorithm};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::CertificateDer;

pub type Provider = Arc<CryptoProvider>;

pub fn default_provider() -> Provider {
    // See <https://docs.rs/rustls/latest/rustls/crypto/struct.CryptoProvider.html#using-the-per-process-default-cryptoprovider>
    if let Some(provider) = CryptoProvider::get_default().cloned() {
        return provider;
    }

    #[cfg(feature = "aws-lc-rs")]
    {
        Arc::new(rustls::crypto::aws_lc_rs::default_provider())
    }
    #[cfg(all(feature = "ring", not(feature = "aws-lc-rs")))]
    {
        Arc::new(rustls::crypto::ring::default_provider())
    }
    #[cfg(not(any(feature = "ring", feature = "aws-lc-rs")))]
    {
        panic!("rustls CryptoProvider::set_default() not called and no 'ring'/'aws-lc-rs' feature enabled.");
    }
}

pub fn sha256(provider: &Provider, cert: &CertificateDer<'_>) -> hash::Output {
    let hash_provider = provider.cipher_suites.iter().find_map(|suite| {
        let hash_provider = suite.tls13()?.common.hash_provider;
        if hash_provider.algorithm() == HashAlgorithm::SHA256 {
            Some(hash_provider)
        } else {
            None
        }
    });
    if let Some(hash_provider) = hash_provider {
        return hash_provider.hash(cert);
    }

    #[cfg(feature = "aws-lc-rs")]
    {
        hash::Output::new(aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, cert).as_ref())
    }
    #[cfg(all(feature = "ring", not(feature = "aws-lc-rs")))]
    {
        return hash::Output::new(ring::digest::digest(&ring::digest::SHA256, cert).as_ref());
    }
    #[cfg(not(any(feature = "ring", feature = "aws-lc-rs")))]
    {
        panic!("No SHA-256 backend available. Ensure your provider exposes SHA-256 or enable 'ring'/'aws-lc-rs' feature.");
    }
}
