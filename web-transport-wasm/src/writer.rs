use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{WritableStream, WritableStreamDefaultWriter};

use crate::{Error, PromiseExt};

// Wrapper around WritableStream
pub struct Writer {
    inner: WritableStreamDefaultWriter,
}

impl Writer {
    pub fn new(stream: &WritableStream) -> Result<Self, Error> {
        let inner = stream.get_writer()?.unchecked_into();
        Ok(Self { inner })
    }

    pub async fn write(&mut self, v: &JsValue) -> Result<(), Error> {
        JsFuture::from(self.inner.write_with_chunk(v)).await?;
        Ok(())
    }

    pub fn close(&mut self) {
        self.inner.close().ignore();
    }

    pub fn abort(&mut self, reason: &str) {
        let str = JsValue::from_str(reason);
        self.inner.abort_with_reason(&str).ignore();
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.inner.release_lock();
    }
}
