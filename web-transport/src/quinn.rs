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
    /// Block until the peer creates a new unidirectional stream.
    ///
    /// Won't return None unless the connection is closed.
    pub async fn accept_uni(&mut self) -> Result<RecvStream, Error> {
        let stream = self.inner.accept_uni().await?;
        Ok(RecvStream::new(stream))
    }

    /// Block until the peer creates a new bidirectional stream.
    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        let (s, r) = self.inner.accept_bi().await?;
        Ok((SendStream::new(s), RecvStream::new(r)))
    }

    /// Open a new bidirectional stream, which may block when there are too many concurrent streams.
    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), Error> {
        Ok(self
            .inner
            .open_bi()
            .await
            .map(|(s, r)| (SendStream::new(s), RecvStream::new(r)))?)
    }

    /// Open a new unidirectional stream, which may block when there are too many concurrent streams.
    pub async fn open_uni(&mut self) -> Result<SendStream, Error> {
        Ok(self.inner.open_uni().await.map(SendStream::new)?)
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
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), Error> {
        // NOTE: This is not async, but we need to make it async to match the wasm implementation.
        Ok(self.inner.send_datagram(payload)?)
    }

    /// The maximum size of a datagram that can be sent.
    pub async fn max_datagram_size(&self) -> usize {
        self.inner.max_datagram_size()
    }

    /// Receive a datagram over the network.
    pub async fn recv_datagram(&mut self) -> Result<Bytes, Error> {
        Ok(self.inner.read_datagram().await?)
    }

    /// Close the connection immediately with a code and reason.
    pub fn close(&mut self, code: u32, reason: &str) {
        self.inner.close(code, reason.as_bytes())
    }

    /// Block until the connection is closed.
    pub async fn closed(&self) -> Error {
        self.inner.closed().await.into()
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

    /// Write *al** of the buffer to the stream, returning the number of bytes written.
    pub async fn write(&mut self, buf: &[u8]) -> Result<(), Error> {
        self.inner.write_all(buf).await?;
        Ok(())
    }

    /// Write all of the given buffer to the stream, advancing the internal position.
    ///
    /// This may be polled to perform partial writes.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<(), Error> {
        while buf.has_remaining() {
            let chunk = buf.chunk();
            self.write(chunk).await?;
            buf.advance(chunk.len());
        }

        Ok(())
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
    ///
    /// The number of bytes read is returned, or None if the stream is closed.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Error> {
        Ok(self.inner.read(buf).await?)
    }

    /// Read some data into the provided buffer.
    ///
    /// The number of bytes read is returned, or None if the stream is closed.
    /// The buffer will be advanced by the number of bytes read.
    pub async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, Error> {
        let dst = buf.chunk_mut();
        let dst = unsafe { &mut *(dst as *mut _ as *mut [u8]) };

        let size = match self.inner.read(dst).await? {
            Some(size) => size,
            None => return Ok(None),
        };

        unsafe { buf.advance_mut(size) };

        Ok(Some(size))
    }

    /// Read the next chunk of data with the provided maximum size.
    ///
    /// This returns a chunk of data instead of copying, which may be more efficient.
    pub async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, Error> {
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

/// A WebTransport error.
///
/// The source can either be a session error or a stream error.
/// TODO This interface is currently not generic.
#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("session error: {0}")]
    Session(#[from] web_transport_quinn::SessionError),

    #[error("write error: {0}")]
    Write(web_transport_quinn::WriteError),

    #[error("read error: {0}")]
    Read(web_transport_quinn::ReadError),
}

impl From<web_transport_quinn::WriteError> for Error {
    fn from(e: web_transport_quinn::WriteError) -> Self {
        match e {
            web_transport_quinn::WriteError::SessionError(e) => Error::Session(e),
            e => Error::Write(e),
        }
    }
}
impl From<web_transport_quinn::ReadError> for Error {
    fn from(e: web_transport_quinn::ReadError) -> Self {
        match e {
            web_transport_quinn::ReadError::SessionError(e) => Error::Session(e),
            e => Error::Read(e),
        }
    }
}
