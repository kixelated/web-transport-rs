use bytes::{Buf, Bytes};

use crate::ErrorCode;

/// A trait describing the "send" actions of a QUIC stream.
#[async_trait::async_trait(?Send)]
pub trait SendStream: Unpin {
    type Error: ErrorCode;

    /// Set the stream's priority relative to other streams on the same connection.
    /// The **highest** priority stream with pending data will be sent first.
    /// Zero is the default value.
    fn priority(&mut self, order: i32);

    /// Send a QUIC reset code.
    fn close(self, code: u32);

    /// Write some of the given buffer to the stream.
    async fn write<B: Buf>(&mut self, buf: &mut B) -> Result<usize, Self::Error>;

    /// Write the entire chunk of bytes to the stream.
    /// More efficient for some implementations, as it avoids a copy
    async fn write_chunk(&mut self, buf: Bytes) -> Result<(), Self::Error>;
}
