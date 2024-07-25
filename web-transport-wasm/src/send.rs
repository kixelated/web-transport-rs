use bytes::{Buf, Bytes};
use js_sys::{Reflect, Uint8Array};
use web_sys::WebTransportSendStream;

use crate::{WriteError, Writer};

pub struct SendStream {
    stream: WebTransportSendStream,
    writer: Writer,
}

impl SendStream {
    pub(super) fn new(stream: WebTransportSendStream) -> Result<Self, WriteError> {
        let writer = Writer::new(&stream)?;
        Ok(Self { stream, writer })
    }

    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        self.writer.write(&Uint8Array::from(buf)).await?;
        Ok(buf.len())
    }

    pub async fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WriteError> {
        let chunk = buf.chunk();
        self.writer.write(&Uint8Array::from(chunk)).await?;
        Ok(chunk.len())
    }

    pub async fn write_chunk(&mut self, buf: Bytes) -> Result<(), WriteError> {
        self.write(&buf).await.map(|_| ())
    }

    pub fn reset(&mut self, reason: &str) {
        self.writer.close(reason);
    }

    pub fn set_priority(&mut self, order: i32) {
        Reflect::set(&self.stream, &"sendOrder".into(), &order.into())
            .expect("failed to set priority");
    }
}
