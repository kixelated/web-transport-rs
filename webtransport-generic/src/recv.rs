use tokio::io::AsyncRead;

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream: AsyncRead + Unpin + Send {
    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32);
}
