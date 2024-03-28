use std::{
    error::Error,
    fmt,
    future::Future,
    ops,
    pin::{pin, Pin},
    task::{ready, Context, Poll},
};

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
    type Error = ReadError;

    /// Send a `STOP_SENDING` QUIC code.
    fn close(mut self, code: u32) {
        quinn::RecvStream::stop(&mut self, VarInt::from_u32(code)).ok();
    }

    fn poll_read_buf<B: bytes::BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>> {
        let dst = buf.chunk_mut();
        let dst = unsafe { &mut *(dst as *mut _ as *mut [u8]) };

        Poll::Ready(
            match ready!(pin!(quinn::RecvStream::read(self, dst)).poll(cx)) {
                Ok(Some(n)) => unsafe {
                    buf.advance_mut(n);
                    Ok(n)
                },
                Ok(None) => Ok(0),
                Err(err) => Err(err.into()),
            },
        )
    }

    fn poll_read_chunk(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<bytes::Bytes>, Self::Error>> {
        Poll::Ready(
            match ready!(pin!(quinn::RecvStream::read_chunk(self, usize::MAX, true)).poll(cx)) {
                Ok(Some(chunk)) => Ok(Some(chunk.bytes)),
                Ok(None) => Ok(None),
                Err(err) => Err(err.into()),
            },
        )
    }
}

#[derive(Clone)]
pub struct ReadError(quinn::ReadError);

impl From<quinn::ReadError> for ReadError {
    fn from(err: quinn::ReadError) -> Self {
        ReadError(err)
    }
}

impl Error for ReadError {}

impl fmt::Debug for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl webtransport_generic::ErrorCode for ReadError {
    fn code(&self) -> Option<u32> {
        match self.0 {
            quinn::ReadError::Reset(code) => TryInto::<u32>::try_into(code.into_inner()).ok(),
            _ => None,
        }
    }
}
