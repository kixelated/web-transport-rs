use std::{ops, pin::Pin};

use quinn::VarInt;
use tokio::io::AsyncWrite;

pub struct SendStream(quinn::SendStream);

impl ops::Deref for SendStream {
    type Target = quinn::SendStream;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for SendStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<quinn::SendStream> for SendStream {
    fn from(stream: quinn::SendStream) -> Self {
        SendStream(stream)
    }
}

impl AsyncWrite for SendStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        quinn::SendStream::poll_write(Pin::new(&mut self.0), cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        quinn::SendStream::poll_flush(Pin::new(&mut self.0), cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        quinn::SendStream::poll_shutdown(Pin::new(&mut self.0), cx)
    }
}

impl webtransport_generic::SendStream for SendStream {
    fn set_priority(&mut self, order: i32) {
        quinn::SendStream::set_priority(self, order).ok();
    }

    fn reset(&mut self, code: u32) {
        quinn::SendStream::reset(self, VarInt::from_u32(code)).ok();
    }
}
