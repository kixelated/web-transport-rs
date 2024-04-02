use std::cmp;

use bytes::{BufMut, Bytes, BytesMut};
use js_sys::Uint8Array;
use web_sys::WebTransportReceiveStream;

use crate::{Reader, WebError};

pub struct RecvStream {
    reader: Reader,
    buffer: BytesMut,
}

impl RecvStream {
    pub fn new(stream: WebTransportReceiveStream) -> Result<Self, WebError> {
        if stream.locked() {
            return Err("locked".into());
        }

        let reader = Reader::new(&stream)?;

        Ok(Self {
            reader,
            buffer: BytesMut::new(),
        })
    }

    async fn read<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, WebError> {
        Ok(self.read_chunk(buf.remaining_mut()).await?.map(|chunk| {
            let size = chunk.len();
            buf.put(chunk);
            size
        }))
    }

    async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, WebError> {
        if !self.buffer.is_empty() {
            let size = cmp::min(max, self.buffer.len());
            let data = self.buffer.split_to(size).freeze();
            return Ok(Some(data));
        }

        let mut data: Bytes = match self.reader.read::<Uint8Array>().await? {
            // TODO can we avoid making a copy here?
            Some(data) => data.to_vec().into(),
            None => return Ok(None),
        };

        if data.len() > max {
            // The chunk is too big; add the tail to the buffer for next read.
            self.buffer.extend_from_slice(&data.split_off(max));
        }

        Ok(Some(data))
    }

    fn close(self, reason: &str) {
        self.reader.close(reason);
    }
}

#[async_trait::async_trait(?Send)]
impl webtransport_generic::RecvStream for RecvStream {
    type Error = WebError;

    async fn read<B: BufMut>(&mut self, buf: &mut B) -> Result<Option<usize>, Self::Error> {
        RecvStream::read(self, buf).await
    }

    async fn read_chunk(&mut self, max: usize) -> Result<Option<Bytes>, Self::Error> {
        RecvStream::read_chunk(self, max).await
    }

    fn close(self, code: u32) {
        RecvStream::close(self, &code.to_string())
    }
}
