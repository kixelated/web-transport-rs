use bytes::{Buf, BufMut};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{RecvStream, SendStream, Session};

/// Trait representing a WebTransport session
pub trait SessionExt: Session + Unpin {
    /// A future that accepts an incoming unidirectional stream.
    fn accept_uni(&mut self) -> AcceptUni<'_, Self> {
        AcceptUni::new(self)
    }

    /// A future that accepts an incoming bidirectional stream.
    fn accept_bidi(&mut self) -> AcceptBidi<'_, Self> {
        AcceptBidi::new(self)
    }

    /// A future that crates a new bidirectional stream.
    fn open_bidi(&mut self) -> OpenBidi<'_, Self> {
        OpenBidi::new(self)
    }

    /// A future that crates a new bidirectional stream.
    fn open_uni(&mut self) -> OpenUni<'_, Self> {
        OpenUni::new(self)
    }
}

pub trait SendStreamExt: SendStream + Unpin {
    /// Attempts to write data into the stream, returing ready when a non-zero number of bytes were written.
    fn send<'a, B: Buf>(&'a mut self, buf: &'a mut B) -> SendBuf<'a, Self, B> {
        SendBuf::new(self, buf)
    }

    /// Finish the sending side of the stream.
    fn finish(&mut self) -> Finish<'_, Self> {
        Finish::new(self)
    }
}

pub trait RecvStreamExt: RecvStream + Unpin {
    /// Return a future that resolves when the next chunk of data is received.
    fn recv<'a, B: BufMut>(&'a mut self, buf: &'a mut B) -> Recv<'a, Self, B> {
        Recv::new(self, buf)
    }
}

// I barely know why this works; I just copied it from futures/tokio.

pub struct AcceptUni<'a, T: ?Sized> {
    conn: &'a mut T,
}

impl<T: ?Sized + Unpin> Unpin for AcceptUni<'_, T> {}

impl<'a, T: Session + ?Sized + Unpin> AcceptUni<'a, T> {
    pub(crate) fn new(conn: &'a mut T) -> Self {
        Self { conn }
    }
}

impl<'a, T> Future for AcceptUni<'a, T>
where
    T: Session + Unpin + ?Sized,
{
    type Output = Result<T::RecvStream, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.conn).poll_accept_uni(cx)
    }
}

pub struct AcceptBidi<'a, T: ?Sized> {
    conn: &'a mut T,
}

impl<T: ?Sized + Unpin> Unpin for AcceptBidi<'_, T> {}

impl<'a, T: Session + ?Sized + Unpin> AcceptBidi<'a, T> {
    pub(crate) fn new(conn: &'a mut T) -> Self {
        Self { conn }
    }
}

impl<'a, T> Future for AcceptBidi<'a, T>
where
    T: Session + Unpin + ?Sized,
{
    type Output = Result<(T::SendStream, T::RecvStream), T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.conn).poll_accept_bidi(cx)
    }
}

pub struct OpenUni<'a, T: ?Sized> {
    conn: &'a mut T,
}

impl<T: ?Sized + Unpin> Unpin for OpenUni<'_, T> {}

impl<'a, T: Session + ?Sized + Unpin> OpenUni<'a, T> {
    pub(crate) fn new(conn: &'a mut T) -> Self {
        Self { conn }
    }
}

impl<'a, T> Future for OpenUni<'a, T>
where
    T: Session + Unpin + ?Sized,
{
    type Output = Result<T::SendStream, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.conn).poll_open_uni(cx)
    }
}

pub struct OpenBidi<'a, T: ?Sized> {
    conn: &'a mut T,
}

impl<T: ?Sized + Unpin> Unpin for OpenBidi<'_, T> {}

impl<'a, T: Session + ?Sized + Unpin> OpenBidi<'a, T> {
    pub(crate) fn new(conn: &'a mut T) -> Self {
        Self { conn }
    }
}

impl<'a, T> Future for OpenBidi<'a, T>
where
    T: Session + Unpin + ?Sized,
{
    type Output = Result<(T::SendStream, T::RecvStream), T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.conn).poll_open_bidi(cx)
    }
}
pub struct SendBuf<'a, T: ?Sized, B: Buf> {
    stream: &'a mut T,
    buf: &'a mut B,
}

impl<T: ?Sized + Unpin, B: Buf> Unpin for SendBuf<'_, T, B> {}

impl<'a, T, B: Buf> SendBuf<'a, T, B>
where
    T: SendStream + Unpin + ?Sized,
    B: Buf,
{
    pub(crate) fn new(stream: &'a mut T, buf: &'a mut B) -> Self {
        Self { stream, buf }
    }
}

impl<'a, T, B> Future for SendBuf<'a, T, B>
where
    T: SendStream + Unpin + ?Sized,
    B: Buf,
{
    type Output = Result<usize, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.stream).poll_send(cx, this.buf)
    }
}

pub struct Finish<'a, T: ?Sized> {
    stream: &'a mut T,
}

impl<T: ?Sized + Unpin> Unpin for Finish<'_, T> {}

impl<'a, T> Finish<'a, T>
where
    T: SendStream + Unpin + ?Sized,
{
    pub(crate) fn new(stream: &'a mut T) -> Self {
        Self { stream }
    }
}

impl<'a, T> Future for Finish<'a, T>
where
    T: SendStream + Unpin + ?Sized,
{
    type Output = Result<(), T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.stream).poll_finish(cx)
    }
}

pub struct Recv<'a, T: ?Sized, B> {
    stream: &'a mut T,
    buf: &'a mut B,
}

impl<T: ?Sized + Unpin, B> Unpin for Recv<'_, T, B> {}

impl<'a, T, B> Recv<'a, T, B>
where
    T: RecvStream + Unpin + ?Sized,
    B: BufMut,
{
    pub(crate) fn new(stream: &'a mut T, buf: &'a mut B) -> Self {
        Self { stream, buf }
    }
}

impl<'a, T, B> Future for Recv<'a, T, B>
where
    T: RecvStream + Unpin + ?Sized,
    B: BufMut,
{
    type Output = Result<Option<usize>, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.stream).poll_recv(cx, &mut this.buf)
    }
}
