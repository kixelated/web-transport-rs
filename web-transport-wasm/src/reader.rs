use js_sys::Reflect;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStream, ReadableStreamDefaultReader, ReadableStreamReadResult};

use crate::WebError;

// Wrapper around ReadableStream
pub struct Reader {
    inner: ReadableStreamDefaultReader,
}

impl Reader {
    pub fn new(stream: &ReadableStream) -> Result<Self, WebError> {
        let inner = stream.get_reader().unchecked_into();
        Ok(Self { inner })
    }

    pub async fn read<T: JsCast>(&mut self) -> Result<Option<T>, WebError> {
        let result: ReadableStreamReadResult = JsFuture::from(self.inner.read()).await?.into();

        if Reflect::get(&result, &"done".into())?.is_truthy() {
            return Ok(None);
        }

        let res = Reflect::get(&result, &"value".into())?.dyn_into()?;
        Ok(Some(res))
    }

    pub fn close(self, reason: &str) {
        let str = JsValue::from_str(reason);
        let _ = self.inner.cancel_with_reason(&str); // ignore the promise
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.inner.cancel(); // ignore the promise
        let _ = self.inner.release_lock();
    }
}
