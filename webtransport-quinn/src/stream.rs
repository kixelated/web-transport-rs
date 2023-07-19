use std::{
    io,
    pin::{pin, Pin},
    task::{ready, Context, Poll},
};

use bytes::{Buf, BufMut, Bytes};
use futures::Future;

use crate::{ReadError, ReadExactError, ReadToEndError, StoppedError, StreamClosed, WriteError};

pub struct SendStream {
    inner: quinn::SendStream,
}

impl SendStream {
    pub(crate) fn new(stream: quinn::SendStream) -> Self {
        Self { inner: stream }
    }

    // Not a varint because we share the error space with HTTP/3.
    pub fn reset(&mut self, code: u32) -> Result<(), StreamClosed> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.reset(code).map_err(Into::into)
    }

    // Returns None if the code is not a valid WebTransport error code.
    pub async fn stopped(&mut self) -> Result<Option<u32>, StoppedError> {
        let code = self.inner.stopped().await?;
        Ok(webtransport_proto::error_from_http3(code.into_inner()))
    }

    // Unfortunately, we have to wrap WriteError for a bunch of functions.
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.inner.write(buf).await.map_err(Into::into)
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError> {
        self.inner.write_all(buf).await.map_err(Into::into)
    }

    pub async fn write_chunks(
        &mut self,
        bufs: &mut [Bytes],
    ) -> Result<quinn_proto::Written, WriteError> {
        self.inner.write_chunks(bufs).await.map_err(Into::into)
    }

    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.inner.write_chunk(buf).await.map_err(Into::into)
    }

    pub async fn write_all_chunks(&mut self, bufs: &mut [Bytes]) -> Result<(), WriteError> {
        self.inner.write_all_chunks(bufs).await.map_err(Into::into)
    }

    pub async fn finish(&mut self) -> Result<(), WriteError> {
        self.inner.finish().await.map_err(Into::into)
    }
}

impl tokio::io::AsyncWrite for SendStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl webtransport_generic::SendStream for SendStream {
    type Error = WriteError;

    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>> {
        let res = pin!(self.write(buf.chunk())).poll(cx);
        if let Poll::Ready(Ok(size)) = res {
            buf.advance(size);
        }

        res.map_err(Into::into)
    }

    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_finish(cx).map_err(Into::into)
    }

    fn reset(&mut self, reset_code: u32) {
        SendStream::reset(self, reset_code).ok();
    }

    fn set_priority(&mut self, order: i32) {
        self.inner.set_priority(order).ok();
    }
}

pub struct RecvStream {
    inner: quinn::RecvStream,
}

impl RecvStream {
    pub(crate) fn new(stream: quinn::RecvStream) -> Self {
        Self { inner: stream }
    }

    // Not a varint because we share the error space with HTTP/3.
    pub fn stop(&mut self, code: u32) -> Result<(), quinn::UnknownStream> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.stop(code)
    }

    // Unfortunately, we have to wrap ReadError for a bunch of functions.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ReadError> {
        self.inner.read(buf).await.map_err(Into::into)
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ReadExactError> {
        self.inner.read_exact(buf).await.map_err(Into::into)
    }

    pub async fn read_chunk(
        &mut self,
        max_length: usize,
        ordered: bool,
    ) -> Result<Option<quinn::Chunk>, ReadError> {
        self.inner
            .read_chunk(max_length, ordered)
            .await
            .map_err(Into::into)
    }

    pub async fn read_chunks(&mut self, bufs: &mut [Bytes]) -> Result<Option<usize>, ReadError> {
        self.inner.read_chunks(bufs).await.map_err(Into::into)
    }

    pub async fn read_to_end(&mut self, size_limit: usize) -> Result<Vec<u8>, ReadToEndError> {
        self.inner.read_to_end(size_limit).await.map_err(Into::into)
    }

    // We purposely don't expose the stream ID or 0RTT because it's not valid with WebTransport
}

impl tokio::io::AsyncRead for RecvStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl webtransport_generic::RecvStream for RecvStream {
    /// The error type that can occur when receiving data.
    type Error = ReadError;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this should return `None`.
    fn poll_recv<B: BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<Option<usize>, Self::Error>> {
        let size = buf.remaining_mut();
        let res = pin!(self.read_chunk(size, true)).poll(cx);

        Poll::Ready(match ready!(res) {
            Ok(Some(chunk)) => {
                let size = chunk.bytes.len();
                buf.put(chunk.bytes);
                Ok(Some(size))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        })
    }

    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32) {
        self.stop(error_code).ok();
    }
}
