use std::{ops, pin::Pin};

use quinn::VarInt;
use tokio::io::{AsyncRead, ReadBuf};

pub struct RecvStream(quinn::RecvStream);

impl ops::Deref for RecvStream {
    type Target = quinn::RecvStream;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for RecvStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<quinn::RecvStream> for RecvStream {
    fn from(stream: quinn::RecvStream) -> Self {
        RecvStream(stream)
    }
}

impl AsyncRead for RecvStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        quinn::RecvStream::poll_read(Pin::new(&mut self.0), cx, buf)
    }
}

impl webtransport_generic::RecvStream for RecvStream {
    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32) {
        quinn::RecvStream::stop(self, VarInt::from_u32(error_code)).ok();
    }
}
