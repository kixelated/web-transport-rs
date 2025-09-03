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

    #[cfg(feature = "ring")]
    {
        Arc::new(rustls::crypto::ring::default_provider())
    }
    #[cfg(all(feature = "aws-lc-rs", not(feature = "ring")))]
    {
        Arc::new(rustls::crypto::aws_lc_rs::default_provider())
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

    let digest = {
        #[cfg(feature = "ring")]
        {
            ring::digest::digest(&ring::digest::SHA256, cert)
        }
        #[cfg(all(feature = "aws-lc-rs", not(feature = "ring")))]
        {
            aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, cert)
        }
    };
    hash::Output::new(digest.as_ref())
}
