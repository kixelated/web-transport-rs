use bytes::{Buf, BufMut};

use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Trait representing a WebTransport session
pub trait Session {
    /// The type produced by `poll_accept_bidi()`
    //type BidiStream: BidiStream;
    /// The type of the sending part of `BidiStream`
    type SendStream: SendStream;
    /// The type produced by `poll_accept_uni()`
    type RecvStream: RecvStream;
    /// Error type yielded by this trait's methods
    type Error: Into<Box<dyn SessionError>>;

    /// Accept an incoming unidirectional stream
    fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::RecvStream, Self::Error>>;

    /// A future that accepts an incoming unidirectional stream.
    fn accept_uni(&mut self) -> AcceptUni<'_, Self>
    where
        Self: Unpin,
    {
        AcceptUni::new(self)
    }

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// A future that accepts an incoming bidirectional stream.
    fn accept_bidi(&mut self) -> AcceptBidi<'_, Self>
    where
        Self: Unpin,
    {
        AcceptBidi::new(self)
    }

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// A future that crates a new bidirectional stream.
    fn open_bidi(&mut self) -> OpenBidi<'_, Self>
    where
        Self: Unpin,
    {
        OpenBidi::new(self)
    }

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// A future that crates a new bidirectional stream.
    fn open_uni(&mut self) -> OpenUni<'_, Self>
    where
        Self: Unpin,
    {
        OpenUni::new(self)
    }

    /// Close the connection immediately
    fn close(&mut self, code: u32, reason: &[u8]);
}

/// Trait that represent an error from the transport layer
pub trait SessionError: Error {
    /// Get the QUIC error code from CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32>;
}

impl<'a, E: SessionError + 'a> From<E> for Box<dyn SessionError + 'a> {
    fn from(err: E) -> Box<dyn SessionError + 'a> {
        Box::new(err)
    }
}

/// A trait describing the "send" actions of a QUIC stream.
pub trait SendStream {
    /// The error type returned by fallible send methods.
    type Error: Into<Box<dyn StreamError>>;

    /// Attempts to write data into the stream, returns the number of bytes written.
    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>>;

    /// Attempts to write data into the stream, returing ready when a non-zero number of bytes were written.
    fn send<'a, B: Buf>(&'a mut self, buf: &'a mut B) -> SendBuf<'a, Self, B>
    where
        Self: Unpin,
    {
        SendBuf::new(self, buf)
    }

    /// Poll to finish the sending side of the stream.
    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    /// Finish the sending side of the stream.
    fn finish(&mut self) -> Finish<'_, Self>
    where
        Self: Unpin,
    {
        Finish::new(self)
    }

    /// Send a QUIC reset code.
    fn reset(&mut self, reset_code: u32);

    /// Set the stream's priority relative to other streams on the same connection.
    /// A lower value will be sent first and zero is the default value.
    fn set_priority(&mut self, order: i32);
}

/// A trait describing the "receive" actions of a QUIC stream.
pub trait RecvStream {
    /// The error type that can occur when receiving data.
    type Error: Into<Box<dyn StreamError>>;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this will return None.
    fn poll_recv<B: BufMut>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<Option<usize>, Self::Error>>;

    /// Return a future that resolves when the next chunk of data is received.
    fn recv<'a, B: BufMut>(&'a mut self, buf: &'a mut B) -> Recv<'a, Self, B>
    where
        Self: Unpin,
    {
        Recv::new(self, buf)
    }

    /// Send a `STOP_SENDING` QUIC code.
    fn stop(&mut self, error_code: u32);
}

/// Trait that represent an error from the transport layer
pub trait StreamError: SessionError {
    /// Get the QUIC error code from RESET_STREAM
    fn stream_error(&self) -> Option<u32>;
}

impl<'a, E: StreamError + 'a> From<E> for Box<dyn StreamError + 'a> {
    fn from(err: E) -> Box<dyn StreamError + 'a> {
        Box::new(err)
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
