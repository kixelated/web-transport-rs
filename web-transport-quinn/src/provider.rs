use std::sync::Arc;

use rustls::crypto::hash::HashAlgorithm;
use rustls::crypto::{self, CryptoProvider};
use rustls::pki_types::CertificateDer;

#[derive(Clone, Debug)]
pub(crate) struct Provider(Arc<CryptoProvider>);

impl Default for Provider {
    fn default() -> Self {
        // See <https://docs.rs/rustls/latest/rustls/crypto/struct.CryptoProvider.html#using-the-per-process-default-cryptoprovider>
        if let Some(provider) = CryptoProvider::get_default() {
            return Self(Arc::clone(provider));
        }

        #[cfg(feature = "ring")]
        {
            Self(Arc::new(rustls::crypto::ring::default_provider()))
        }
        #[cfg(all(feature = "aws-lc-rs", not(feature = "ring")))]
        {
            Self(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        }
    }
}

impl Provider {
    pub fn provider(&self) -> Arc<CryptoProvider> {
        Arc::clone(&self.0)
    }

    pub fn sha256(&self, cert: &CertificateDer<'_>) -> crypto::hash::Output {
        let hash_provider = self.0.cipher_suites.iter().find_map(|suite| {
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
        crypto::hash::Output::new(digest.as_ref())
    }
}
