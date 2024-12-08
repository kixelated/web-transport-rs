use js_sys::{Object, Reflect, Uint8Array};
use url::Url;
use wasm_bindgen_futures::JsFuture;
use web_sys::{WebTransport, WebTransportOptions};

use crate::{Error, Session};

pub use web_sys::WebTransportCongestionControl as CongestionControl;

/// Build a session with the given URL and options.
#[derive(Default, Debug)]
pub struct Client {
    // Check https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.WebTransportOptions.html
    options: WebTransportOptions,
}

// Check https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.WebTransportOptions.html
impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    /// Determine if the client/server is allowed to pool connections.
    /// (Hint) Don't set it to true.
    pub fn allow_pooling(self, val: bool) -> Self {
        self.options.set_allow_pooling(val);
        self
    }

    /// Determine if HTTP/2 is a valid fallback.
    pub fn require_unreliable(self, val: bool) -> Self {
        self.options.set_require_unreliable(val);
        self
    }

    /// Hint at the required congestion control algorithm
    pub fn congestion_control(self, control: CongestionControl) -> Self {
        self.options.set_congestion_control(control);
        self
    }

    /// Supply sha256 hashes for accepted certificates, instead of using a root CA
    pub fn server_certificate_hashes(self, hashes: Vec<Vec<u8>>) -> Self {
        // expected: [ { algorithm: "sha-256", value: hashValue }, ... ]
        let hashes = hashes
            .into_iter()
            .map(|hash| {
                let hash = Uint8Array::from(&hash[..]);
                let obj = Object::new();
                Reflect::set(&obj, &"algorithm".into(), &"sha-256".into()).unwrap();
                Reflect::set(&obj, &"value".into(), &hash.into()).unwrap();
                obj
            })
            .collect::<js_sys::Array>();

        self.options.set_server_certificate_hashes(&hashes);
        self
    }

    /// Connect once the builder is configured.
    pub async fn connect(&self, url: &Url) -> Result<Session, Error> {
        let inner = WebTransport::new_with_options(url.as_str(), &self.options)?;
        JsFuture::from(inner.ready()).await?;

        Ok(inner.into())
    }
}
