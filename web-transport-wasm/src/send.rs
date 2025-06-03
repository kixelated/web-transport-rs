use bytes::Buf;
use js_sys::{Reflect, Uint8Array};
use web_sys::WebTransportSendStream;

use crate::Error;
use web_streams::TypedWriter;

/// A stream of bytes sent to the remote peer.
pub struct SendStream {
    stream: WebTransportSendStream,
    writer: TypedWriter<Uint8Array>,
}

impl SendStream {
    pub(super) fn new(stream: WebTransportSendStream) -> Result<Self, Error> {
        let writer = TypedWriter::new(&stream)?;
        Ok(Self { stream, writer })
    }

    /// Write *all* of the given bytes to the stream.
    pub async fn write(&mut self, buf: &[u8]) -> Result<(), Error> {
        let mut buf = std::io::Cursor::new(buf);
        self.write_buf(&mut buf).await
    }

    /// Write the given buffer to the stream.
    ///
    /// Advances the buffer by the number of bytes written.
    /// May be polled/timed out to perform partial writes.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<(), Error> {
        while buf.has_remaining() {
            let chunk = buf.chunk();
            self.writer.write(&Uint8Array::from(chunk)).await?;
            buf.advance(chunk.len());
        }

        Ok(())
    }

    /// Send an immediate reset code, closing the stream with an error.
    pub fn reset(&mut self, reason: &str) {
        self.writer.abort(reason);
    }

    /// Mark the stream as finished.
    ///
    /// This is automatically called on Drop, but can be called manually.
    pub fn finish(&mut self) -> Result<(), Error> {
        self.writer.close();
        Ok(())
    }

    /// Set the stream's priority.
    ///
    /// Streams with **higher** values are sent first, but are not guaranteed to arrive first.
    pub fn set_priority(&mut self, priority: i32) {
        Reflect::set(&self.stream, &"sendOrder".into(), &priority.into())
            .expect("failed to set priority");
    }

    /// Block until the stream has been closed and return the error code, if any.
    pub async fn closed(&self) -> Result<Option<u8>, Error> {
        let err = match self.writer.closed().await {
            Ok(()) => return Ok(None),
            Err(err) => Error::from(err),
        };

        // If it's a WebTransportError, we can extract the error code.
        if let Error::Stream(err) = &err {
            if let Some(code) = err.stream_error_code() {
                return Ok(Some(code));
            }
        }

        Err(err)
    }
}

impl Drop for SendStream {
    /// Close the stream with a FIN.
    fn drop(&mut self) {
        self.writer.close();
    }
}

#[cfg(feature = "tokio")]
mod tokio_impl {
    use super::*;
    use std::io::{Error};
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncWrite;


    impl AsyncWrite for SendStream {
        fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::result::Result<usize, Error>> {
            Pin::new(&mut self.writer).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Error>> {
            Pin::new(&mut self.writer).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Error>> {
            Pin::new(&mut self.writer).poll_shutdown(cx)
        }
    }
}
