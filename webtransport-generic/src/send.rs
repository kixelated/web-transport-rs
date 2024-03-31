use std::{
    future::{poll_fn, Future},
    task::{Context, Poll},
};

use bytes::Buf;

use crate::ErrorCode;

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream: Unpin + Send {
    type Error: ErrorCode;

    /// Set the stream's priority relative to other streams on the same connection.
    /// The **highest** priority stream with pending data will be sent first.
    /// Zero is the default value.
    fn priority(&mut self, order: i32);

    /// Send a QUIC reset code.
    fn close(self, code: u32);

    /// Attempt to write some of the given buffer to the stream.
    fn poll_write(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Self::Error>>;

    // A helper future that calls poll_write
    fn write(&mut self, buf: &[u8]) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        poll_fn(|cx| self.poll_write(cx, buf))
    }

    fn poll_write_buf<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>>;

    // A helper future that calls poll_write
    fn write_buf<B: Buf + Send>(
        &mut self,
        buf: &mut B,
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        poll_fn(|cx| self.poll_write_buf(cx, buf))
    }

    // TODO add write_chunk to avoid making a copy?
}
