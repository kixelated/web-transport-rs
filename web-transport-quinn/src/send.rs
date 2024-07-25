use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;

use crate::{StoppedError, StreamClosed, WriteError};

/// A stream that can be used to send bytes. See [`quinn::SendStream`].
///
/// This wrapper is mainly needed for error codes, which is unfortunate.
/// WebTransport uses u32 error codes and they're mapped in a reserved HTTP/3 error space.
#[derive(Debug)]
pub struct SendStream {
    inner: quinn::SendStream,
}

impl SendStream {
    pub(crate) fn new(stream: quinn::SendStream) -> Self {
        Self { inner: stream }
    }

    /// Abruptly reset the stream with the provided error code. See [`quinn::SendStream::reset`].
    /// This is a u32 with WebTransport because we share the error space with HTTP/3.
    pub fn reset(&mut self, code: u32) -> Result<(), StreamClosed> {
        let code = web_transport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.reset(code).map_err(Into::into)
    }

    /// Wait until the stream has been stopped and return the error code. See [`quinn::SendStream::stopped`].
    /// Unlike Quinn, this returns None if the code is not a valid WebTransport error code.
    pub async fn stopped(&mut self) -> Result<Option<u32>, StoppedError> {
        let code = self.inner.stopped().await?;
        Ok(web_transport_proto::error_from_http3(code.into_inner()))
    }

    // Unfortunately, we have to wrap WriteError for a bunch of functions.

    /// Write some data to the stream, returning the size written. See [`quinn::SendStream::write`].
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.inner.write(buf).await.map_err(Into::into)
    }

    /// Write all of the data to the stream. See [`quinn::SendStream::write_all`].
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError> {
        self.inner.write_all(buf).await.map_err(Into::into)
    }

    /// Write chunks of data to the stream. See [`quinn::SendStream::write_chunks`].
    pub async fn write_chunks(
        &mut self,
        bufs: &mut [Bytes],
    ) -> Result<quinn_proto::Written, WriteError> {
        self.inner.write_chunks(bufs).await.map_err(Into::into)
    }

    /// Write a chunk of data to the stream. See [`quinn::SendStream::write_chunk`].
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.inner.write_chunk(buf).await.map_err(Into::into)
    }

    /// Write all of the chunks of data to the stream. See [`quinn::SendStream::write_all_chunks`].
    pub async fn write_all_chunks(&mut self, bufs: &mut [Bytes]) -> Result<(), WriteError> {
        self.inner.write_all_chunks(bufs).await.map_err(Into::into)
    }

    /// Wait until all of the data has been written to the stream. See [`quinn::SendStream::finish`].
    pub async fn finish(&mut self) -> Result<(), WriteError> {
        self.inner.finish().await.map_err(Into::into)
    }

    pub fn set_priority(&self, order: i32) -> Result<(), StreamClosed> {
        self.inner.set_priority(order).map_err(Into::into)
    }

    pub fn priority(&self) -> Result<i32, StreamClosed> {
        self.inner.priority().map_err(Into::into)
    }

    /// Return a unique stream identifier.
    ///
    /// NOTE: This is not part of the WebTransport API, but is useful for debugging.
    /// This is not the same as QUIC's stream ID; don't rely on it for anything.
    pub fn id(&self) -> u64 {
        self.inner.id().index()
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
