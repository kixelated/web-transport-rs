use bytes::Buf;
use js_sys::{Reflect, Uint8Array};
use web_sys::WebTransportSendStream;

use crate::{WebError, Writer};

pub struct SendStream {
    stream: WebTransportSendStream,
    writer: Writer,
}

impl SendStream {
    pub fn new(stream: WebTransportSendStream) -> Result<Self, WebError> {
        let writer = Writer::new(&stream)?;
        Ok(Self { stream, writer })
    }

    pub async fn write<B: Buf>(&mut self, buf: &mut B) -> Result<usize, WebError> {
        let chunk = buf.chunk();
        self.writer.write(&Uint8Array::from(chunk)).await?;
        Ok(chunk.len())
    }

    pub fn close(self, reason: &str) {
        self.writer.close(reason);
    }

    pub fn priority(&mut self, order: i32) {
        Reflect::set(&self.stream, &"sendOrder".into(), &order.into())
            .expect("failed to set priority");
    }
}
