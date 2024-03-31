use std::{
    future::{poll_fn, Future},
    task::{Context, Poll},
};

use bytes::{BufMut, Bytes};

use crate::ErrorCode;

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream: Unpin + Send {
    type Error: ErrorCode;

    /// Send a `STOP_SENDING` QUIC code.
    fn close(self, code: u32);

    /// Attempt to read from the stream into the given buffer.
    fn poll_read_buf<B: BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>>;

    // A helper future that calls poll_read_buf.
    fn read_buf<B: BufMut + Send>(
        &mut self,
        buf: &mut B,
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        poll_fn(move |cx| self.poll_read_buf(cx, buf))
    }

    /// Attempt to write the given buffer to the stream.
    fn poll_read_chunk(&mut self, cx: &mut Context<'_>)
        -> Poll<Result<Option<Bytes>, Self::Error>>;

    // A helper future that calls poll_read_chunk.
    fn read_chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>, Self::Error>> + Send {
        poll_fn(|cx| self.poll_read_chunk(cx))
    }
}
