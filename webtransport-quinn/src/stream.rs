use std::ops::{Deref, DerefMut};

pub struct SendStream {
    inner: quinn::SendStream,
}

impl SendStream {
    pub(crate) fn new(stream: quinn::SendStream) -> Self {
        Self { inner: stream }
    }
}

impl Deref for SendStream {
    type Target = quinn::SendStream;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SendStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub struct RecvStream {
    inner: quinn::RecvStream,
}

impl RecvStream {
    pub(crate) fn new(stream: quinn::RecvStream) -> Self {
        Self { inner: stream }
    }
}

impl Deref for RecvStream {
    type Target = quinn::RecvStream;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for RecvStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
