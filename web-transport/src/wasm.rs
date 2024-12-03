use bytes::{Buf, BufMut, Bytes};

// Export the Wasm implementation to simplify Cargo.toml
pub use web_transport_wasm as wasm;

pub struct Client {
    inner: web_transport_wasm::Client,
}

impl Client {
    pub fn new() -> Self {
        Self {
            inner: web_transport_wasm::Client::new(),
        }
    }

    pub fn low_latency(self) -> Self {
        Self {
            inner: self.inner.low_latency(),
        }
    }

    pub fn server_certificate_hashes(self, hashes: Vec<Vec<u8>>) -> Self {
        Self {
            inner: self.inner.server_certificate_hashes(hashes),
        }
    }

    pub async fn connect(&self, url: &Url) -> Result<Session, Error> {
        Ok(self.inner.connect(url).await?.into())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Session(web_transport_wasm::Session);

impl Session {
    pub async fn accept_uni(&mut self) -> Result<RecvStream, Error> {
        let stream = self.0.accept_uni().await?;
        Ok(RecvStream(stream))
    }

    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let (s, r) = self.0.accept_bi().await?;
        Ok((SendStream(s), RecvStream(r)))
    }

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let (s, r) = self.0.open_bi().await?;
        Ok((SendStream(s), RecvStream(r)))
    }

    pub async fn open_uni(&mut self) -> Result<SendStream, Error> {
        self.0.open_uni().await.map(SendStream)
    }

    /// Close the connection immediately
    pub fn close(&mut self, code: u32, reason: &str) {
        self.0.close(code, reason)
    }

    pub async fn closed(&self) -> Error {
        self.0.closed().await
    }

    /// Send a datagram.
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), Error> {
        self.0.send_datagram(payload).await
    }

    pub async fn recv_datagram(&mut self) -> Result<Bytes, Error> {
        self.0.recv_datagram().await
    }
}

impl From<web_transport_wasm::Session> for Session {
    fn from(session: web_transport_wasm::Session) -> Self {
        Session(session)
    }
}

pub struct SendStream(web_transport_wasm::SendStream);

impl SendStream {
    /// Write all of the given data to the stream.
    pub async fn write(&mut self, buf: &[u8]) -> Result<(), Error> {
        self.0.write(buf).await
    }

    /// Write some of the given buffer to the stream.
    ///
    /// Advances the internal position by the number of bytes written.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<(), Error> {
        self.0.write_buf(buf).await
    }

    pub fn set_priority(&mut self, order: i32) {
        self.0.set_priority(order)
    }

    /// Send a QUIC reset code.
    pub fn reset(&mut self, code: u32) {
        self.0.reset(&code.to_string())
    }
}

pub struct RecvStream(web_transport_wasm::RecvStream);

impl RecvStream {
    /// Attempt to read a chunk of unbuffered data.
    pub async fn read(&mut self, max: usize) -> Result<Option<Bytes>, Error> {
        self.0.read(max).await
    }

    /// Attempt to read from the stream into the given buffer.
    pub async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, Error> {
        self.0.read_buf(buf).await
    }

    /// Send a `STOP_SENDING` QUIC code.
    pub fn stop(&mut self, code: u32) {
        self.0.stop(&code.to_string())
    }
}

pub type Error = web_transport_wasm::Error;
