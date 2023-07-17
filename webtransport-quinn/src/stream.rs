use std::{
    ops::{Deref, DerefMut},
    pin::pin,
    task::{ready, Context, Poll},
};

use bytes::{Buf, BufMut};
use futures::Future;

use crate::{RecvError, SendError};

pub struct SendStream {
    inner: quinn::SendStream,
}

impl SendStream {
    pub(crate) fn new(stream: quinn::SendStream) -> Self {
        Self { inner: stream }
    }

    // TODO need to overload any methods that return a WriteError to fix the error code...

    // Not a varint because we share the error space with HTTP/3.
    pub fn reset(&mut self, code: u32) -> Result<(), quinn::UnknownStream> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.reset(code)
    }

    pub async fn stopped(&mut self) -> Result<u32, quinn::StoppedError> {
        todo!("stopped");
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

impl webtransport_generic::SendStream for SendStream {
    type Error = SendError;

    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>> {
        let res = pin!(self.write(buf.chunk())).poll(cx);
        if let Poll::Ready(Ok(size)) = res {
            buf.advance(size);
        }

        res.map_err(Into::into)
    }

    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_finish(cx).map_err(Into::into)
    }

    fn reset(&mut self, reset_code: u32) {
        SendStream::reset(self, reset_code).ok();
    }

    fn set_priority(&mut self, order: i32) {
        self.inner.set_priority(order).ok();
    }
}

pub struct RecvStream {
    inner: quinn::RecvStream,
}

impl RecvStream {
    pub(crate) fn new(stream: quinn::RecvStream) -> Self {
        Self { inner: stream }
    }

    // Not a varint because we share the error space with HTTP/3.
    pub fn stop(&mut self, code: u32) -> Result<(), quinn::UnknownStream> {
        let code = webtransport_proto::error_to_http3(code);
        let code = quinn::VarInt::try_from(code).unwrap();
        self.inner.stop(code)
    }

    // TODO need to overload any methods that implement ReadError to fix the error code...
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

impl webtransport_generic::RecvStream for RecvStream {
    /// The error type that can occur when receiving data.
    type Error = RecvError;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this should return `None`.
    fn poll_recv<B: BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<Option<usize>, Self::Error>> {
        let size = buf.remaining_mut();
        let res = pin!(self.read_chunk(size, true)).poll(cx);

        Poll::Ready(match ready!(res) {
            Ok(Some(chunk)) => {
                let size = chunk.bytes.len();
                buf.put(chunk.bytes);
                Ok(Some(size))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        })
    }

    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32) {
        self.stop(error_code).ok();
    }
}
