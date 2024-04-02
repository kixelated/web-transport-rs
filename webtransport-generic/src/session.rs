use crate::{ErrorCode, RecvStream, SendStream};

/// Trait representing a WebTransport session.
///
/// The Session can be cloned to produce multiple handles and each method is &self, mirroing the Quinn API.
/// This is overly permissive, but otherwise Quinn would need an extra Arc<Mutex<Session>> wrapper which would hurt performance.
#[async_trait::async_trait(?Send)]
pub trait Session: Clone + Unpin {
    type SendStream: SendStream;
    type RecvStream: RecvStream;
    type Error: ErrorCode;

    async fn accept_uni(&mut self) -> Result<Self::RecvStream, Self::Error>;
    async fn accept_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error>;
    async fn open_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error>;
    async fn open_uni(&mut self) -> Result<Self::SendStream, Self::Error>;

    /// Close the connection immediately
    fn close(self, code: u32, reason: &str);

    /// A future that blocks until the connection is closed.
    async fn closed(&self) -> Self::Error;

    /// Send a datagram.
    async fn send_datagram(&mut self, payload: bytes::Bytes) -> Result<(), Self::Error>;

    /// A helper to make poll_recv_datagram async
    async fn recv_datagram(&mut self) -> Result<bytes::Bytes, Self::Error>;
}
