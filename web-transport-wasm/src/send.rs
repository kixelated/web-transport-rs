use bytes::Buf;
use js_sys::{Reflect, Uint8Array};
use web_sys::WebTransportSendStream;

use crate::{Error, Writer};

/// A stream of bytes sent to the remote peer.
pub struct SendStream {
    stream: WebTransportSendStream,
    writer: Writer,
}

impl SendStream {
    pub(super) fn new(stream: WebTransportSendStream) -> Result<Self, Error> {
        let writer = Writer::new(&stream)?;
        Ok(Self { stream, writer })
    }

    /// Write some of the given buffer to the stream.
    ///
    /// Returns the non-zero number of bytes written.
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.writer.write(&Uint8Array::from(buf)).await?;
        Ok(buf.len())
    }

    /// Write all of the given buffer to the stream.
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
        let mut buf = std::io::Cursor::new(buf);
        self.write_all_buf(&mut buf).await
    }

    /// Write some of the given buffer to the stream.
    ///
    /// Returns the non-zero number of bytes written.
    /// Advances the buffer by the number of bytes written.
    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<usize, Error> {
        let size = self.write(buf.chunk()).await?;
        buf.advance(size);

        Ok(size)
    }

    /// Write all of the given buffer to the stream.
    ///
    /// Advances the buffer by the number of bytes written, including any partial writes.
    pub async fn write_all_buf<B: Buf>(&mut self, buf: &mut B) -> Result<(), Error> {
        while buf.has_remaining() {
            self.write_buf(buf).await?;
        }
        Ok(())
    }

    /// Send an immediate reset code, closing the stream with an error.
    pub fn reset(&mut self, reason: &str) {
        self.writer.abort(reason);
    }

    /// Set the stream's priority.
    ///
    /// Streams with **higher** values are sent first, but are not guaranteed to arrive first.
    pub fn set_priority(&mut self, priority: i32) {
        Reflect::set(&self.stream, &"sendOrder".into(), &priority.into())
            .expect("failed to set priority");
    }
}

impl Drop for SendStream {
    /// Close the stream with a FIN.
    fn drop(&mut self) {
        self.writer.close();
    }
}
