use std::{error::Error, fmt, ops, pin::Pin};

use bytes::{BufMut, Bytes};
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

#[async_trait::async_trait(?Send)]
impl webtransport_generic::RecvStream for RecvStream {
    type Error = ReadError;

    /// Send a `STOP_SENDING` QUIC code.
    fn close(mut self, code: u32) {
        quinn::RecvStream::stop(&mut self, VarInt::from_u32(code)).ok();
    }

    async fn read<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, Self::Error> {
        let dst = buf.chunk_mut();
        let mut dst = unsafe { &mut *(dst as *mut _ as *mut [u8]) };

        quinn::RecvStream::read(self, &mut dst)
            .await
            .map(|res| {
                res.map(|n| {
                    unsafe { buf.advance_mut(n) }
                    n
                })
            })
            .map_err(Into::into)
    }

    async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, Self::Error> {
        quinn::RecvStream::read_chunk(self, max, true)
            .await
            .map(|chunk| chunk.map(|chunk| chunk.bytes))
            .map_err(Into::into)
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
