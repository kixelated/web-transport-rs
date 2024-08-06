use wasm_bindgen::prelude::*;

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("webtransport session error: {0:?}")]
    Session(web_sys::WebTransportError),

    #[error("webtransport stream error: {0:?}")]
    Stream(web_sys::WebTransportError),

    #[error("unknown error: {0:?}")]
    Unknown(JsValue),
}

impl Error {
    pub fn code(&self) -> Option<u8> {
        match self {
            Error::Session(e) | Error::Stream(e) => e.stream_error_code(),
            Error::Unknown(_) => None,
        }
    }
}

impl From<JsValue> for Error {
    fn from(v: JsValue) -> Self {
        if let Some(e) = v.dyn_ref::<web_sys::WebTransportError>().cloned() {
            match e.source() {
                web_sys::WebTransportErrorSource::Stream => Error::Stream(e),
                web_sys::WebTransportErrorSource::Session => Error::Session(e),
                _ => Error::Unknown(v),
            }
        } else {
            Error::Unknown(v)
        }
    }
}

pub type Result = std::result::Result<(), Error>;

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
