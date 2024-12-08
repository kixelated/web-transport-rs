use bytes::Buf;
use js_sys::{Reflect, Uint8Array};
use web_sys::WebTransportSendStream;

use crate::Error;
use web_streams::Writer;

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
