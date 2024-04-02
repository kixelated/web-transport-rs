use bytes::{Buf, Bytes};
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

    fn close(self, reason: &str) {
        self.writer.close(reason);
    }

    fn priority(&mut self, order: i32) {
        Reflect::set(&self.stream, &"sendOrder".into(), &order.into())
            .expect("failed to set priority");
    }
}

#[async_trait::async_trait(?Send)]
impl webtransport_generic::SendStream for SendStream {
    type Error = WebError;

    async fn write<B: Buf>(&mut self, buf: &mut B) -> Result<usize, Self::Error> {
        SendStream::write(self, buf).await
    }

    async fn write_chunk(&mut self, mut buf: Bytes) -> Result<(), Self::Error> {
        SendStream::write(self, &mut buf).await.map(|_| ())
    }

    fn close(self, code: u32) {
        SendStream::close(self, &code.to_string());
    }

    fn priority(&mut self, order: i32) {
        SendStream::priority(self, order);
    }
}
