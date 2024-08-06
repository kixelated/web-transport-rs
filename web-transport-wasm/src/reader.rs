use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStream, ReadableStreamDefaultReader, ReadableStreamReadResult};

use crate::{Error, PromiseExt};

// Wrapper around ReadableStream
pub struct Reader {
    inner: ReadableStreamDefaultReader,
}

impl Reader {
    pub fn new(stream: &ReadableStream) -> Result<Self, Error> {
        let inner = stream.get_reader().unchecked_into();
        Ok(Self { inner })
    }

    pub async fn read<T: JsCast>(&mut self) -> Result<Option<T>, Error> {
        let result: ReadableStreamReadResult = JsFuture::from(self.inner.read()).await?.into();

        if Reflect::get(&result, &"done".into())?.is_truthy() {
            return Ok(None);
        }

        let res = Reflect::get(&result, &"value".into())?.unchecked_into();

        Ok(Some(res))
    }

    pub fn abort(&mut self, reason: &str) {
        let str = JsValue::from_str(reason);
        self.inner.cancel_with_reason(&str).ignore();
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.inner.release_lock();
    }
}
