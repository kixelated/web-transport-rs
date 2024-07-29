use bytes::{Buf, BufMut, Bytes};

// Export the Quinn implementation to simplify Cargo.toml
pub use web_transport_quinn as quinn;

/// A WebTransport Session, able to accept/create streams and send/recv datagrams.
///
/// The session can be cloned to create multiple handles.
/// The session will be closed with on drop.
#[derive(Clone)]
pub struct Session {
    inner: web_transport_quinn::Session,
}

impl Session {
    pub async fn connect(_url: &str) -> Result<Self, SessionError> {
        unimplemented!("TODO use a default Quinn config")
    }

    /// Block until the peer creates a new unidirectional stream.
    pub async fn accept_uni(&mut self) -> Result<RecvStream, SessionError> {
        self.inner.accept_uni().await.map(RecvStream::new)
    }

    /// Block until the peer creates a new bidirectional stream.
    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.inner
            .accept_bi()
            .await
            .map(|(s, r)| (SendStream::new(s), RecvStream::new(r)))
    }

    /// Open a new bidirectional stream, which may block when there are too many concurrent streams.
    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.inner
            .open_bi()
            .await
            .map(|(s, r)| (SendStream::new(s), RecvStream::new(r)))
    }

    /// Open a new unidirectional stream, which may block when there are too many concurrent streams.
    pub async fn open_uni(&mut self) -> Result<SendStream, SessionError> {
        self.inner.open_uni().await.map(SendStream::new)
    }

    /// Send a datagram over the network.
    ///
    /// QUIC datagrams may be dropped for any reason:
    /// - Network congestion.
    /// - Random packet loss.
    /// - Payload is larger than `max_datagram_size()`
    /// - Peer is not receiving datagrams.
    /// - Peer has too many outstanding datagrams.
    /// - ???
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), SessionError> {
        // NOTE: This is not async, but we need to make it async to match the wasm implementation.
        self.inner.send_datagram(payload)
    }

    /// The maximum size of a datagram that can be sent.
    pub async fn max_datagram_size(&self) -> usize {
        self.inner.max_datagram_size()
    }

    /// Receive a datagram over the network.
    pub async fn recv_datagram(&mut self) -> Result<Bytes, SessionError> {
        self.inner.read_datagram().await
    }

    /// Close the connection immediately with a code and reason.
    pub fn close(&mut self, code: u32, reason: &str) {
        self.inner.close(code, reason.as_bytes())
    }

    /// Block until the connection is closed.
    pub async fn closed(&self) -> Result<(), SessionError> {
        // TODO correctly parse the code/reason
        Err(self.inner.closed().await)
    }
}

/// Convert a `web_transport_quinn::Session` into a `web_transport::Session`.
impl From<web_transport_quinn::Session> for Session {
    fn from(session: web_transport_quinn::Session) -> Self {
        Session { inner: session }
    }
}

/// An outgoing stream of bytes to the peer.
///
/// QUIC streams have flow control, which means the send rate is limited by the peer's receive window.
/// The stream will be closed with a graceful FIN when dropped.
pub struct SendStream {
    inner: web_transport_quinn::SendStream,
}

impl SendStream {
    fn new(inner: web_transport_quinn::SendStream) -> Self {
        Self { inner }
    }

    /// Write some of the buffer to the stream, potentailly blocking on flow control.
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.inner.write(buf).await
    }

    /// Write some of the given buffer to the stream, potentially blocking on flow control.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WriteError> {
        let size = self.inner.write(buf.chunk()).await?;
        buf.advance(size);
        Ok(size)
    }

    /// Write the entire chunk of bytes to the stream.
    ///
    /// More efficient for some implementations, as it avoids a copy
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.inner.write_chunk(buf).await
    }

    /// Set the stream's priority.
    ///
    /// Streams with lower values will be sent first, but are not guaranteed to arrive first.
    pub fn set_priority(&mut self, order: i32) {
        self.inner.set_priority(order).ok();
    }

    /// Send an immediate reset code, closing the stream.
    pub fn reset(&mut self, code: u32) {
        self.inner.reset(code).ok();
    }
}

/// An incoming stream of bytes from the peer.
///
/// All bytes are flushed in order and the stream is flow controlled.
/// The stream will be closed with STOP_SENDING code=0 when dropped.
pub struct RecvStream {
    inner: web_transport_quinn::RecvStream,
}

impl RecvStream {
    fn new(inner: web_transport_quinn::RecvStream) -> Self {
        Self { inner }
    }

    /// Read some data into the provided buffer.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ReadError> {
        self.inner.read(buf).await
    }

    /// Read some data into the provided buffer.
    pub async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Result<bool, ReadError> {
        let dst = buf.chunk_mut();
        let dst = unsafe { &mut *(dst as *mut _ as *mut [u8]) };

        let size = match self.inner.read(dst).await? {
            Some(size) => size,
            None => return Ok(false),
        };

        unsafe { buf.advance_mut(size) };

        Ok(true)
    }

    /// Read the next chunk of data with the provided maximum size.
    ///
    /// More efficient for some implementations, as it avoids a copy
    pub async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, ReadError> {
        Ok(self
            .inner
            .read_chunk(max, true)
            .await?
            .map(|chunk| chunk.bytes))
    }

    /// Send a `STOP_SENDING` QUIC code.
    pub fn stop(&mut self, code: u32) {
        self.inner.stop(code).ok();
    }
}

/// A [Session] error
pub type SessionError = web_transport_quinn::SessionError;

/// A [SendStream] error
pub type WriteError = web_transport_quinn::WriteError;

/// A [RecvStream] error
pub type ReadError = web_transport_quinn::ReadError;
