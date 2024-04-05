use bytes::{Buf, BufMut, Bytes};

#[derive(Clone)]
pub struct Session(pub web_transport_quinn::Session);

impl Session {
    pub async fn accept_uni(&mut self) -> Result<RecvStream, SessionError> {
        unimplemented!()
    }

    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        unimplemented!()
    }

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        unimplemented!()
    }

    pub async fn open_uni(&mut self) -> Result<(SendStream), SessionError> {
        unimplemented!()
    }

    /// Close the connection immediately
    pub fn close(self, code: u32, reason: &str) {
        unimplemented!()
    }

    /// A future that blocks until the connection is closed.
    pub async fn closed(&self) -> SessionError {
        unimplemented!()
    }

    /// Send a datagram.
    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), SessionError> {
        unimplemented!()
    }

    /// A helper to make poll_recv_datagram async
    pub async fn recv_datagram(&mut self) -> Result<Bytes, SessionError> {
        unimplemented!()
    }
}

impl From<web_transport_quinn::Session> for Session {
    fn from(session: web_transport_quinn::Session) -> Self {
        Session(session)
    }
}

pub struct SendStream(pub web_transport_quinn::SendStream);

impl SendStream {
    pub fn priority(&mut self, order: i32) {
        unimplemented!()
    }

    /// Send a QUIC reset code.
    pub fn close(self, code: u32) {
        unimplemented!()
    }

    /// Write some of the given buffer to the stream.
    pub async fn write<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WriteError> {
        unimplemented!()
    }

    /// Write the entire chunk of bytes to the stream.
    /// More efficient for some implementations, as it avoids a copy
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        unimplemented!()
    }
}

pub struct RecvStream(pub web_transport_quinn::RecvStream);

impl RecvStream {
    /// Send a `STOP_SENDING` QUIC code.
    pub fn close(self, code: u32) {
        unimplemented!()
    }

    /// Attempt to read from the stream into the given buffer.
    pub async fn read<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, ReadError> {
        unimplemented!()
    }

    /// Attempt to read a chunk of unbuffered data.
    /// More efficient for some implementations, as it avoids a copy
    pub async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, ReadError> {
        unimplemented!()
    }
}

pub type SessionError = web_transport_quinn::SessionError;
pub type WriteError = web_transport_quinn::WriteError;
pub type ReadError = web_transport_quinn::ReadError;
