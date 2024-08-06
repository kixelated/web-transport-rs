use bytes::Bytes;
use js_sys::{Object, Reflect, Uint8Array};
use url::Url;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    WebTransport, WebTransportBidirectionalStream, WebTransportCloseInfo,
    WebTransportCongestionControl, WebTransportOptions, WebTransportSendStream,
};

use crate::{Error, Reader, RecvStream, SendStream, Writer};

#[derive(Clone)]
pub struct Session {
    inner: WebTransport,
}

impl Session {
    pub fn build(url: Url) -> SessionBuilder {
        SessionBuilder::new(url)
    }

    pub async fn connect(url: Url) -> Result<Session, Error> {
        Self::build(url).connect().await
    }

    pub async fn accept_uni(&mut self) -> Result<RecvStream, Error> {
        let mut reader = Reader::new(&self.inner.incoming_unidirectional_streams())?;

        match reader.read().await? {
            Some(stream) => Ok(RecvStream::new(stream)?),
            None => Err(self.closed().await),
        }
    }

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

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let stream: WebTransportBidirectionalStream =
            JsFuture::from(self.inner.create_bidirectional_stream())
                .await?
                .into();

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    pub async fn open_uni(&mut self) -> Result<SendStream, Error> {
        let stream: WebTransportSendStream =
            JsFuture::from(self.inner.create_unidirectional_stream())
                .await?
                .into();

        let send = SendStream::new(stream)?;
        Ok(send)
    }

    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), Error> {
        let mut writer = Writer::new(&self.inner.datagrams().writable())?;
        writer.write(&Uint8Array::from(payload.as_ref())).await?;
        Ok(())
    }

    pub async fn recv_datagram(&mut self) -> Result<Bytes, Error> {
        let mut reader = Reader::new(&self.inner.datagrams().readable())?;
        let data: Uint8Array = reader.read().await?.unwrap_or_default();
        Ok(data.to_vec().into())
    }

    pub fn close(&mut self, code: u32, reason: &str) {
        let mut info = WebTransportCloseInfo::new();
        info.close_code(code);
        info.reason(reason);
        self.inner.close_with_close_info(&info);
    }

    pub async fn closed(&self) -> Error {
        self.closed_inner().await.unwrap_err()
    }

    async fn closed_inner(&self) -> Result<(), Error> {
        let result: js_sys::Object = JsFuture::from(self.inner.closed()).await?.into();

        // For some reason, WebTransportCloseInfo only contains setters
        let code = Reflect::get(&result, &"closeCode".into())?
            .as_f64()
            .unwrap() as u8;
        let reason = Reflect::get(&result, &"reason".into())?
            .as_string()
            .unwrap();

        let mut options = web_sys::WebTransportErrorOptions::new();
        options.source(web_sys::WebTransportErrorSource::Session);
        options.stream_error_code(Some(code));

        let err = web_sys::WebTransportError::new_with_message_and_options(&reason, &options)?;
        Err(Error::Session(err))
    }
}

pub struct SessionBuilder {
    url: Url,
    options: WebTransportOptions,
}

// Check https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.WebTransportOptions.html
impl SessionBuilder {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            options: WebTransportOptions::new(),
        }
    }

    /// Determine if the client/server is allowed to pool connections.
    /// (Hint) Don't set it to true.
    pub fn allow_pooling(mut self, val: bool) -> Self {
        self.options.allow_pooling(val);
        self
    }

    /// Determine if HTTP/2 is a valid fallback.
    pub fn require_unreliable(mut self, val: bool) -> Self {
        self.options.require_unreliable(val);
        self
    }

    /// Hint at the required congestion control algorithm
    pub fn congestion_control(mut self, control: CongestionControl) -> Self {
        self.options.congestion_control(control);
        self
    }

    /// Supply sha256 hashes for accepted certificates, instead of using a root CA
    pub fn server_certificate_hashes(mut self, hashes: Vec<Vec<u8>>) -> Self {
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

        self.options.server_certificate_hashes(&hashes);
        self
    }

    pub async fn connect(self) -> Result<Session, Error> {
        let inner = WebTransport::new_with_options(self.url.as_ref(), &self.options)?;
        JsFuture::from(inner.ready()).await?;

        Ok(Session { inner })
    }
}

pub type CongestionControl = WebTransportCongestionControl;
