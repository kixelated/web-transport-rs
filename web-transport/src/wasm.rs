use bytes::{Buf, BufMut, Bytes};

#[derive(Clone)]
pub struct Session(web_transport_wasm::Session);

impl Session {
    pub async fn accept_uni(&mut self) -> Result<RecvStream, SessionError> {
        self.0
            .accept_uni()
            .await
            .map(RecvStream)
            .map_err(Into::into)
    }

    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.0
            .accept_bi()
            .await
            .map(|(s, r)| (SendStream(s), RecvStream(r)))
            .map_err(Into::into)
    }

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.0
            .open_bi()
            .await
            .map(|(s, r)| (SendStream(s), RecvStream(r)))
            .map_err(Into::into)
    }

    pub async fn open_uni(&mut self) -> Result<SendStream, SessionError> {
        self.0.open_uni().await.map(SendStream).map_err(Into::into)
    }

    /// Close the connection immediately
    pub fn close(self, code: u32, reason: &str) {
        self.0.close(code, reason)
    }

    pub async fn closed(&self) -> SessionError {
        self.0.closed().await.into()
    }

    /// Send a datagram.
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), SessionError> {
        self.0.send_datagram(payload).await.map_err(Into::into)
    }

    pub async fn recv_datagram(&mut self) -> Result<Bytes, SessionError> {
        self.0.recv_datagram().await.map_err(Into::into)
    }
}

impl From<web_transport_wasm::Session> for Session {
    fn from(session: web_transport_wasm::Session) -> Self {
        Session(session)
    }
}

pub struct SendStream(web_transport_wasm::SendStream);

impl SendStream {
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.0.write(buf).await.map_err(Into::into)
    }

    /// Write some of the given buffer to the stream.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WriteError> {
        self.0.write_buf(buf).await.map_err(Into::into)
    }

    /// Write the entire chunk of bytes to the stream.
    /// More efficient for some implementations, as it avoids a copy
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.0.write_chunk(buf).await.map_err(Into::into)
    }

    pub fn set_priority(&mut self, order: i32) {
        self.0.set_priority(order)
    }

    /// Send a QUIC reset code.
    pub fn reset(self, code: u32) {
        self.0.reset(&code.to_string())
    }
}

pub struct RecvStream(web_transport_wasm::RecvStream);

impl RecvStream {
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ReadError> {
        self.0.read(buf).await.map_err(Into::into)
    }

    /// Attempt to read from the stream into the given buffer.
    pub async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Result<bool, ReadError> {
        self.0.read_buf(buf).await.map_err(Into::into)
    }

    /// Attempt to read a chunk of unbuffered data.
    pub async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, ReadError> {
        self.0.read_chunk(max).await.map_err(Into::into)
    }

    /// Send a `STOP_SENDING` QUIC code.
    pub fn stop(self, code: u32) {
        self.0.stop(&code.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SessionError(#[from] web_transport_wasm::WebError);

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct WriteError(#[from] web_transport_wasm::WebError);

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ReadError(#[from] web_transport_wasm::WebError);
