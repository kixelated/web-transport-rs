use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{WritableStream, WritableStreamDefaultWriter};

use crate::WebError;

// Wrapper around WritableStream
pub struct Writer {
    inner: WritableStreamDefaultWriter,
}

impl Writer {
    pub fn new(stream: &WritableStream) -> Result<Self, WebError> {
        let inner = stream.get_writer()?.unchecked_into();
        Ok(Self { inner })
    }

    pub async fn write(&mut self, v: &JsValue) -> Result<(), WebError> {
        JsFuture::from(self.inner.write_with_chunk(v)).await?;
        Ok(())
    }

    pub fn close(self, reason: &str) {
        let str = JsValue::from_str(reason);
        let _ = self.inner.abort_with_reason(&str); // ignore the promise
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.inner.close(); // ignore the promise
        self.inner.release_lock();
    }
}
