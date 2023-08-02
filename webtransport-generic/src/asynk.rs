use bytes::{Buf, BufMut};

use std::future::Future;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use super::{RecvStream, SendStream, Session};

/// Trait representing a WebTransport session
pub trait AsyncSession: Session + Send + Unpin {
    /// A future that accepts an incoming unidirectional stream.
    fn accept_uni(&self) -> AcceptUni<'_, Self> {
        AcceptUni { session: self }
    }

    /// A future that accepts an incoming bidirectional stream.
    fn accept_bi(&self) -> AcceptBi<'_, Self> {
        AcceptBi { session: self }
    }

    /// A future that crates a new bidirectional stream.
    fn open_bi(&self) -> OpenBi<'_, Self> {
        OpenBi { session: self }
    }

    /// A future that crates a new unidirectional stream.
    fn open_uni(&self) -> OpenUni<'_, Self> {
        OpenUni { session: self }
    }

    /// A future that blocks until the connection is closed.
    fn closed(&self) -> Closed<'_, Self> {
        Closed { session: self }
    }
}

pub trait AsyncSendStream: SendStream + Send + Unpin {
    /// Attempts to write data into the stream, returing ready when a non-zero number of bytes were written.
    fn send<'a, B: Buf>(&'a mut self, buf: &'a mut B) -> SendBuf<'a, Self, B> {
        SendBuf { stream: self, buf }
    }

    fn send_all<'a, B: Buf>(&'a mut self, buf: &'a mut B) -> SendAll<'a, Self, B> {
        SendAll { stream: self, buf }
    }
}

pub trait AsyncRecvStream: RecvStream + Send + Unpin {
    /// Return a future that resolves when the next chunk of data is received.
    fn recv<'a, B: BufMut>(&'a mut self, buf: &'a mut B) -> Recv<'a, Self, B> {
        Recv { stream: self, buf }
    }

    fn recv_all<'a, B: BufMut>(&'a mut self, buf: &'a mut B) -> RecvAll<'a, Self, B> {
        RecvAll { stream: self, buf }
    }
}

impl<S: Session + Unpin + Send> AsyncSession for S {}
impl<S: SendStream + Unpin + Send> AsyncSendStream for S {}
impl<R: RecvStream + Unpin + Send> AsyncRecvStream for R {}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct AcceptUni<'a, S: ?Sized> {
    session: &'a S,
}

impl<'a, S> Future for AcceptUni<'a, S>
where
    S: AsyncSession,
{
    type Output = Result<S::RecvStream, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_accept_uni(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct AcceptBi<'a, S: ?Sized> {
    session: &'a S,
}

impl<'a, S> Future for AcceptBi<'a, S>
where
    S: AsyncSession,
{
    type Output = Result<(S::SendStream, S::RecvStream), S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_accept_bi(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OpenUni<'a, S: ?Sized> {
    session: &'a S,
}

impl<'a, S> Future for OpenUni<'a, S>
where
    S: AsyncSession,
{
    type Output = Result<S::SendStream, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_open_uni(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OpenBi<'a, S: ?Sized> {
    session: &'a S,
}

impl<'a, S> Future for OpenBi<'a, S>
where
    S: AsyncSession,
{
    type Output = Result<(S::SendStream, S::RecvStream), S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_open_bi(cx)
    }
}
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendBuf<'a, S: ?Sized, B: Buf> {
    stream: &'a mut S,
    buf: &'a mut B,
}

impl<'a, S, B> Future for SendBuf<'a, S, B>
where
    S: AsyncSendStream,
    B: Buf,
{
    type Output = Result<usize, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.stream).poll_send(cx, this.buf)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendAll<'a, S: ?Sized, B: Buf> {
    stream: &'a mut S,
    buf: &'a mut B,
}

impl<'a, S, B> Future for SendAll<'a, S, B>
where
    S: AsyncSendStream,
    B: Buf,
{
    type Output = Result<(), S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let mut stream = Pin::new(&mut this.stream);

        while !this.buf.has_remaining() {
            match ready!(stream.poll_send(cx, this.buf)) {
                // Not valid but handle it anyway.
                Ok(0) => return Poll::Pending,

                // Continue looping until the buffer is empty.
                Ok(_n) => {}

                // Fatal error
                Err(e) => return Poll::Ready(Err(e)),
            }
        }

        Poll::Ready(Ok(()))
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Recv<'a, S: ?Sized, B> {
    stream: &'a mut S,
    buf: &'a mut B,
}

impl<'a, S, B> Future for Recv<'a, S, B>
where
    S: AsyncRecvStream,
    B: BufMut,
{
    type Output = Result<Option<usize>, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.stream).poll_recv(cx, &mut this.buf)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RecvAll<'a, S: ?Sized, B> {
    stream: &'a mut S,
    buf: &'a mut B,
}

impl<'a, S, B> Future for RecvAll<'a, S, B>
where
    S: AsyncRecvStream,
    B: BufMut,
{
    type Output = Result<Option<usize>, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let mut stream = Pin::new(&mut this.stream);

        loop {
            match ready!(stream.poll_recv(cx, &mut this.buf)) {
                // This is invalid but handle it just in case
                Ok(Some(0)) => return Poll::Pending,

                // Keep reading more data.
                Ok(Some(_n)) => {}

                // No more data left.
                Ok(None) => return Poll::Ready(Ok(None)),

                // Fatal error.
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Closed<'a, S: ?Sized> {
    session: &'a S,
}

impl<'a, S> Future for Closed<'a, S>
where
    S: AsyncSession,
{
    type Output = S::Error;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_closed(cx)
    }
}
