use bytes::{BufMut, Bytes};

use crate::ErrorCode;

/// A trait describing the "receive" actions of a QUIC stream.
#[async_trait::async_trait(?Send)]
pub trait RecvStream: Unpin {
    type Error: ErrorCode;

    /// Send a `STOP_SENDING` QUIC code.
    fn close(self, code: u32);

    /// Attempt to read from the stream into the given buffer.
    async fn read<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, Self::Error>;

    /// Attempt to read a chunk of unbuffered data.
    /// More efficient for some implementations, as it avoids a copy
    async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, Self::Error>;
}
