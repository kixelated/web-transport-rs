use tokio::io::AsyncWrite;

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream: AsyncWrite + Send + Unpin {
    /// Send a QUIC reset code.
    fn reset(&mut self, reset_code: u32);

    /// Set the stream's priority relative to other streams on the same connection.
    /// A lower value will be sent first and zero is the default value.
    fn set_priority(&mut self, order: i32);
}
