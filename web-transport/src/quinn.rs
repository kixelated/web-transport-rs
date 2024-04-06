use bytes::{Buf, BufMut, Bytes};

#[derive(Clone)]
pub struct Session(web_transport_quinn::Session);

impl Session {
    pub async fn accept_uni(&mut self) -> Result<RecvStream, SessionError> {
        self.0.accept_uni().await.map(RecvStream)
    }

    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.0
            .accept_bi()
            .await
            .map(|(s, r)| (SendStream(s), RecvStream(r)))
    }

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        self.0
            .open_bi()
            .await
            .map(|(s, r)| (SendStream(s), RecvStream(r)))
    }

    pub async fn open_uni(&mut self) -> Result<SendStream, SessionError> {
        self.0.open_uni().await.map(SendStream)
    }

    /// Send a datagram.
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), SessionError> {
        // NOTE: This is not async, but we need to make it async to match the wasm implementation.
        self.0.send_datagram(payload)
    }

    pub async fn recv_datagram(&mut self) -> Result<Bytes, SessionError> {
        self.0.read_datagram().await
    }

    /// Close the connection immediately
    pub fn close(self, code: u32, reason: &str) {
        self.0.close(code, reason.as_bytes())
    }

    pub async fn closed(&self) -> SessionError {
        self.0.closed().await
    }
}

impl From<web_transport_quinn::Session> for Session {
    fn from(session: web_transport_quinn::Session) -> Self {
        Session(session)
    }
}

pub struct SendStream(web_transport_quinn::SendStream);

impl SendStream {
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.0.write(buf).await
    }

    /// Write some of the given buffer to the stream.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WriteError> {
        let size = self.0.write(buf.chunk()).await?;
        buf.advance(size);
        Ok(size)
    }

    /// Write the entire chunk of bytes to the stream.
    /// More efficient for some implementations, as it avoids a copy
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.0.write_chunk(buf).await
    }

    pub fn set_priority(&mut self, order: i32) {
        self.0.set_priority(order).ok();
    }

    /// Send a QUIC reset code.
    pub fn reset(mut self, code: u32) {
        self.0.reset(code).ok();
    }
}

pub struct RecvStream(web_transport_quinn::RecvStream);

impl RecvStream {
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ReadError> {
        self.0.read(buf).await
    }

    /// Attempt to read from the stream into the given buffer.
    pub async fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Result<bool, ReadError> {
        let dst = buf.chunk_mut();
        let dst = unsafe { &mut *(dst as *mut _ as *mut [u8]) };

        let size = match self.0.read(dst).await? {
            Some(size) => size,
            None => return Ok(false),
        };

        unsafe { buf.advance_mut(size) };

        Ok(true)
    }

    /// Attempt to read a chunk of unbuffered data.
    /// More efficient for some implementations, as it avoids a copy
    pub async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, ReadError> {
        Ok(self.0.read_chunk(max, true).await?.map(|chunk| chunk.bytes))
    }

    /// Send a `STOP_SENDING` QUIC code.
    pub fn stop(mut self, code: u32) {
        self.0.stop(code).ok();
    }
}

pub type SessionError = web_transport_quinn::SessionError;
pub type WriteError = web_transport_quinn::WriteError;
pub type ReadError = web_transport_quinn::ReadError;
