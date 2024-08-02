use wasm_bindgen::prelude::*;

#[derive(Clone, Debug, thiserror::Error)]
#[error("web error: {0:?}")]
pub struct WebError(js_sys::Error);

impl From<js_sys::Error> for WebError {
    fn from(e: js_sys::Error) -> Self {
        Self(e)
    }
}

impl From<wasm_bindgen::JsValue> for WebError {
    fn from(e: wasm_bindgen::JsValue) -> Self {
        Self(e.into())
    }
}

pub trait WebErrorExt<T> {
    fn throw(self) -> Result<T, WebError>;
}

impl<T, E: Into<WebError>> WebErrorExt<T> for Result<T, E> {
    fn throw(self) -> Result<T, WebError> {
        self.map_err(Into::into)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("read error: {0:?}")]
pub struct ReadError(#[from] WebError);

#[derive(Clone, Debug, thiserror::Error)]
#[error("write error: {0:?}")]
pub struct WriteError(#[from] WebError);

#[derive(Clone, Debug, thiserror::Error)]
pub enum SessionError {
    // TODO distinguish between different kinds of errors
    #[error("read error: {0}")]
    Read(#[from] ReadError),

    #[error("write error: {0}")]
    Write(#[from] WriteError),

    #[error("web error: {0}")]
    Web(#[from] WebError),
}

pub(crate) trait PromiseExt {
    fn ignore(self);
}

impl PromiseExt for js_sys::Promise {
    // Ignore the result of the promise by using an empty catch.
    fn ignore(self) {
        let closure = Closure::wrap(Box::new(|_: JsValue| {}) as Box<dyn FnMut(JsValue)>);
        let _ = self.catch(&closure);
        closure.forget();
    }
}
