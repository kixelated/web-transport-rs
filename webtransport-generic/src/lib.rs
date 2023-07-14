// Coming from https://github.com/hyperium/h3, the goal is to
// do a PR with the changes afterwards

use bytes::Buf;

use std::error::Error;
use std::task::{Context, Poll};

type ErrorCode = u32; // NOTE: smaller than QUIC

/// Trait representing a QUIC connection.
pub trait Connection {
    /// The type produced by `poll_accept_bidi()`
    //type BidiStream: BidiStream;
    /// The type of the sending part of `BidiStream`
    type SendStream: SendStream;
    /// The type produced by `poll_accept_uni()`
    type RecvStream: RecvStream;
    /// Error type yielded by this trait's methods
    type Error: Into<Box<dyn Error>>;

    /// Accept an incoming unidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::RecvStream>, Self::Error>>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<(Self::SendStream, Self::RecvStream)>, Self::Error>>;

    /// Poll the connection to create a new bidirectional stream.
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
    fn close(&mut self, code: ErrorCode, reason: &[u8]);
}

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream {
    /// The error type returned by fallible send methods.
    type Error: Into<Box<dyn Error>>;

    /// Attempts to write data into the stream, returns the number of bytes written.
    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>>;

    /// Poll to finish the sending side of the stream.
    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    /// Send a QUIC reset code.
    fn reset(&mut self, reset_code: ErrorCode);
}

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream {
    /// The type of `Buf` for data received on this stream.
    type Buf: Buf + Send;
    /// The error type that can occur when receiving data.
    type Error: Into<Box<dyn Error>>;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this should return `None`.
    fn poll_data(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Self::Buf>, Self::Error>>;

    /// Send a `STOP_SENDING` QUIC code.
    fn stop_sending(&mut self, error_code: ErrorCode);
}

/*
pub trait BidiStream {
    /// The type for the send half.
    type SendStream: SendStream;
    /// The type for the receive half.
    type RecvStream: RecvStream;

    /// Split this stream into two halves.
    fn split(self) -> (Self::SendStream, Self::RecvStream);
}
*/
