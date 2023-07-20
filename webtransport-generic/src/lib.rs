use bytes::{Buf, BufMut};

use std::error::Error;

use std::task::{Context, Poll};

mod ext;
pub use ext::*;

/// Trait representing a WebTransport session
pub trait Session {
    /// The type produced by `poll_accept_bidi()`
    //type BidiStream: BidiStream;
    /// The type of the sending part of `BidiStream`
    type SendStream: SendStream;
    /// The type produced by `poll_accept_uni()`
    type RecvStream: RecvStream;
    /// Error type yielded by this trait's methods
    type Error: SessionError;

    /// Accept an incoming unidirectional stream
    fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::RecvStream, Self::Error>>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    #[allow(clippy::type_complexity)]
    fn poll_accept_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new bidirectional stream.
    #[allow(clippy::type_complexity)]
    fn poll_open_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// Close the connection immediately
    fn close(&mut self, code: u32, reason: &[u8]);
}

/// Trait that represent an error from the transport layer
pub trait SessionError: Error + Send + Sync + 'static {
    /// Get the QUIC error code from CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32>;
}

impl<'a, E: SessionError + 'a> From<E> for Box<dyn SessionError + 'a> {
    fn from(err: E) -> Box<dyn SessionError + 'a> {
        Box::new(err)
    }
}

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream {
    /// The error type returned by fallible send methods.
    type Error: StreamError;

    /// Attempts to write data into the stream, returns the number of bytes written.
    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>>;

    /// Poll to finish the sending side of the stream.
    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    /// Send a QUIC reset code.
    fn reset(&mut self, reset_code: u32);

    /// Set the stream's priority relative to other streams on the same connection.
    /// A lower value will be sent first and zero is the default value.
    fn set_priority(&mut self, order: i32);
}

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream {
    /// The error type that can occur when receiving data.
    type Error: StreamError;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this will return None.
    fn poll_recv<B: BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<Option<usize>, Self::Error>>;

    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32);
}

/// Trait that represent an error from the transport layer
pub trait StreamError: SessionError + Send + Sync + 'static {
    /// Get the QUIC error code from RESET_STREAM
    fn stream_error(&self) -> Option<u32>;
}

impl<'a, E: StreamError + 'a> From<E> for Box<dyn StreamError + 'a> {
    fn from(err: E) -> Box<dyn StreamError + 'a> {
        Box::new(err)
    }
}
