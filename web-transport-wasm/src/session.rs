use bytes::Bytes;
use js_sys::{Object, Reflect, Uint8Array};
use url::Url;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    WebTransport, WebTransportBidirectionalStream, WebTransportCloseInfo,
    WebTransportCongestionControl, WebTransportOptions, WebTransportSendStream,
};

use crate::{Error, Reader, RecvStream, SendStream, Writer};

/// A session represents a connection between a client and a server.
///
/// This is the main entry point for creating new streams and sending datagrams.
/// The session can be closed by either endpoint with an error code and reason.
///
/// The session can be cloned to create multiple handles.
/// However, handles cannot (currently) accept/open the same type of stream.
#[derive(Clone)]
pub struct Session {
    inner: WebTransport,
}

impl Session {
    /// Create a new session builder with the given URL.
    pub fn build(url: Url) -> SessionBuilder {
        SessionBuilder::new(url)
    }

    /// Connect to the given URL with the default options.
    pub async fn connect(url: Url) -> Result<Session, Error> {
        Self::build(url).connect().await
    }

    /// Accept a new unidirectional stream from the peer.
    pub async fn accept_uni(&mut self) -> Result<RecvStream, Error> {
        let mut reader = Reader::new(&self.inner.incoming_unidirectional_streams())?;

        match reader.read().await? {
            Some(stream) => Ok(RecvStream::new(stream)?),
            None => Err(self.closed().await),
        }
    }

    /// Accept a new bidirectional stream from the peer.
    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let mut reader = Reader::new(&self.inner.incoming_bidirectional_streams())?;

        let stream: WebTransportBidirectionalStream = match reader.read().await? {
            Some(stream) => stream,
            None => return Err(self.closed().await),
        };

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    /// Creates a new bidirectional stream.
    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let stream: WebTransportBidirectionalStream =
            JsFuture::from(self.inner.create_bidirectional_stream())
                .await?
                .into();

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    /// Creates a new unidirectional stream.
    pub async fn open_uni(&mut self) -> Result<SendStream, Error> {
        let stream: WebTransportSendStream =
            JsFuture::from(self.inner.create_unidirectional_stream())
                .await?
                .into();

        let send = SendStream::new(stream)?;
        Ok(send)
    }

    /// Send a datagram over the network.
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), Error> {
        let mut writer = Writer::new(&self.inner.datagrams().writable())?;
        writer.write(&Uint8Array::from(payload.as_ref())).await?;
        Ok(())
    }

    /// Receive a datagram over the network.
    pub async fn recv_datagram(&mut self) -> Result<Bytes, Error> {
        let mut reader = Reader::new(&self.inner.datagrams().readable())?;
        let data: Uint8Array = reader.read().await?.unwrap_or_default();
        Ok(data.to_vec().into())
    }

    /// Close the session with the given error code and reason.
    pub fn close(&mut self, code: u32, reason: &str) {
        let info = WebTransportCloseInfo::new();
        info.set_close_code(code);
        info.set_reason(reason);
        self.inner.close_with_close_info(&info);
    }

    /// Block until the session is closed and return the error.
    pub async fn closed(&self) -> Error {
        self.closed_inner().await.unwrap_err()
    }

    async fn closed_inner(&self) -> Result<(), Error> {
        let info: WebTransportCloseInfo = JsFuture::from(self.inner.closed()).await?.into();
        let reason = info.get_reason().unwrap_or_default();

        let options = web_sys::WebTransportErrorOptions::new();
        options.set_source(web_sys::WebTransportErrorSource::Session);

        if let Ok(code) = info.get_close_code().map(u8::try_from).transpose() {
            options.set_stream_error_code(code);
        }

        let err = web_sys::WebTransportError::new_with_message_and_options(&reason, &options)?;
        Err(Error::Session(err))
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Session {}

/// Build a session with the given URL and options.
pub struct SessionBuilder {
    url: Url,
    options: WebTransportOptions,
}

// Check https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.WebTransportOptions.html
impl SessionBuilder {
    /// Create a new builder with the given URL.
    pub fn new(url: Url) -> Self {
        Self {
            url,
            options: WebTransportOptions::new(),
        }
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
    pub async fn connect(self) -> Result<Session, Error> {
        let inner = WebTransport::new_with_options(self.url.as_ref(), &self.options)?;
        JsFuture::from(inner.ready()).await?;

        Ok(Session { inner })
    }
}

/// A type of congestion control algorithm.
pub type CongestionControl = WebTransportCongestionControl;
