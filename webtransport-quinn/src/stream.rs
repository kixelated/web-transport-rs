use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;

use crate::{ReadError, ReadExactError, ReadToEndError, StoppedError, StreamClosed, WriteError};

/// A stream that can be used to send bytes. See [`quinn::SendStream`].
///
/// This wrapper is mainly needed for error codes, which is unfortunate.
/// WebTransport uses u32 error codes and they're mapped in a reserved HTTP/3 error space.
#[derive(Debug)]
pub struct SendStream {
    stream: quinn::SendStream,
}

impl SendStream {
    pub(crate) fn new(stream: quinn::SendStream) -> Self {
        Self { stream }
    }

    /// Abruptly reset the stream with the provided error code. See [`quinn::SendStream::reset`].
    /// This is a u32 with WebTransport because we share the error space with HTTP/3.
    pub fn reset(&mut self, code: u32) -> Result<(), StreamClosed> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.stream.reset(code).map_err(Into::into)
    }

    /// Wait until the stream has been stopped and return the error code. See [`quinn::SendStream::stopped`].
    /// Unlike Quinn, this returns None if the code is not a valid WebTransport error code.
    pub async fn stopped(&mut self) -> Result<Option<u32>, StoppedError> {
        let code = self.stream.stopped().await?;
        Ok(webtransport_proto::error_from_http3(code.into_inner()))
    }

    // Unfortunately, we have to wrap WriteError for a bunch of functions.

    /// Write some data to the stream, returning the size written. See [`quinn::SendStream::write`].
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.stream.write(buf).await.map_err(Into::into)
    }

    /// Write all of the data to the stream. See [`quinn::SendStream::write_all`].
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), WriteError> {
        self.stream.write_all(buf).await.map_err(Into::into)
    }

    /// Write chunks of data to the stream. See [`quinn::SendStream::write_chunks`].
    pub async fn write_chunks(
        &mut self,
        bufs: &mut [Bytes],
    ) -> Result<quinn_proto::Written, WriteError> {
        self.stream.write_chunks(bufs).await.map_err(Into::into)
    }

    /// Write a chunk of data to the stream. See [`quinn::SendStream::write_chunk`].
    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.stream.write_chunk(buf).await.map_err(Into::into)
    }

    /// Write all of the chunks of data to the stream. See [`quinn::SendStream::write_all_chunks`].
    pub async fn write_all_chunks(&mut self, bufs: &mut [Bytes]) -> Result<(), WriteError> {
        self.stream.write_all_chunks(bufs).await.map_err(Into::into)
    }

    /// Wait until all of the data has been written to the stream. See [`quinn::SendStream::finish`].
    pub async fn finish(&mut self) -> Result<(), WriteError> {
        self.stream.finish().await.map_err(Into::into)
    }

    pub fn set_priority(&self, order: i32) -> Result<(), StreamClosed> {
        self.stream.set_priority(order).map_err(Into::into)
    }

    pub fn priority(&self) -> Result<i32, StreamClosed> {
        self.stream.priority().map_err(Into::into)
    }
}

impl tokio::io::AsyncWrite for SendStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

impl webtransport_generic::SendStream for SendStream {
    fn reset(&mut self, reset_code: u32) {
        SendStream::reset(self, reset_code).ok();
    }

    fn set_priority(&mut self, order: i32) {
        SendStream::set_priority(self, order).ok();
    }
}

/// A stream that can be used to recieve bytes. See [`quinn::RecvStream`].
#[derive(Debug)]
pub struct RecvStream {
    inner: quinn::RecvStream,
}

impl RecvStream {
    pub(crate) fn new(stream: quinn::RecvStream) -> Self {
        Self { inner: stream }
    }

    /// Tell the other end to stop sending data with the given error code. See [`quinn::RecvStream::stop`].
    /// This is a u32 with WebTransport since it shares the error space with HTTP/3.
    pub fn stop(&mut self, code: u32) -> Result<(), quinn::UnknownStream> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.stop(code)
    }

    // Unfortunately, we have to wrap ReadError for a bunch of functions.

    /// Read some data into the buffer and return the amount read. See [`quinn::RecvStream::read`].
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ReadError> {
        self.inner.read(buf).await.map_err(Into::into)
    }

    /// Fill the entire buffer with data. See [`quinn::RecvStream::read_exact`].
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ReadExactError> {
        self.inner.read_exact(buf).await.map_err(Into::into)
    }

    /// Read a chunk of data from the stream. See [`quinn::RecvStream::read_chunk`].
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

    /// Read chunks of data from the stream. See [`quinn::RecvStream::read_chunks`].
    pub async fn read_chunks(&mut self, bufs: &mut [Bytes]) -> Result<Option<usize>, ReadError> {
        self.inner.read_chunks(bufs).await.map_err(Into::into)
    }

    /// Read until the end of the stream or the limit is hit. See [`quinn::RecvStream::read_to_end`].
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
    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32) {
        self.stop(error_code).ok();
    }
}
