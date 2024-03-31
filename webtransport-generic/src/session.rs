use std::future::{poll_fn, Future};
use std::task::{Context, Poll};

use crate::{ErrorCode, RecvStream, SendStream};

/// Trait representing a WebTransport session.
///
/// The Session can be cloned to produce multiple handles and each method is &self, mirroing the Quinn API.
/// This is overly permissive, but otherwise Quinn would need an extra Arc<Mutex<Session>> wrapper which would hurt performance.
pub trait Session: Clone + Send + Sync + Unpin {
    type SendStream: SendStream;
    type RecvStream: RecvStream;
    type Error: ErrorCode;

    /// Accept an incoming unidirectional stream
    fn poll_accept_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::RecvStream, Self::Error>>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    #[allow(clippy::type_complexity)]
    fn poll_accept_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new bidirectional stream.
    #[allow(clippy::type_complexity)]
    fn poll_open_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// Close the connection immediately
    fn close(&self, code: u32, reason: &[u8]);

    /// Check if the connection is closed, returing the error if it is.
    fn poll_closed(&self, cx: &mut Context<'_>) -> Poll<Self::Error>;

    /// Check if there's a new datagram to read.
    fn poll_recv_datagram(&self, cx: &mut Context<'_>) -> Poll<Result<bytes::Bytes, Self::Error>>;

    /// Send a datagram.
    fn send_datagram(&self, payload: bytes::Bytes) -> Result<(), Self::Error>;

    /// A future that accepts an incoming unidirectional stream.
    fn accept_uni(&self) -> impl Future<Output = Result<Self::RecvStream, Self::Error>> + Send {
        poll_fn(|cx| self.poll_accept_uni(cx))
    }

    /// A future that accepts an incoming bidirectional stream.
    fn accept_bi(
        &self,
    ) -> impl Future<Output = Result<(Self::SendStream, Self::RecvStream), Self::Error>> + Send
    {
        poll_fn(|cx| self.poll_accept_bi(cx))
    }

    /// A future that crates a new bidirectional stream.
    fn open_bi(
        &self,
    ) -> impl Future<Output = Result<(Self::SendStream, Self::RecvStream), Self::Error>> + Send
    {
        poll_fn(|cx| self.poll_open_bi(cx))
    }

    /// A future that crates a new unidirectional stream.
    fn open_uni(&self) -> impl Future<Output = Result<Self::SendStream, Self::Error>> + Send {
        poll_fn(|cx| self.poll_open_uni(cx))
    }

    /// A future that blocks until the connection is closed.
    fn closed(&self) -> impl Future<Output = Self::Error> + Send {
        poll_fn(|cx| self.poll_closed(cx))
    }

    /// A helper to make poll_recv_datagram async
    fn recv_datagram(&self) -> impl Future<Output = Result<bytes::Bytes, Self::Error>> + Send {
        poll_fn(|cx| self.poll_recv_datagram(cx))
    }
}
