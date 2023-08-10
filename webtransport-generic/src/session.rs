use std::error::Error;
use std::future::Future;

use std::pin::Pin;
use std::task::{Context, Poll};

use super::{RecvStream, SendStream};

/// Trait representing a WebTransport session.
///
/// The Session can be cloned to produce multiple handles and each method is &self, mirroing the Quinn API.
/// This is overly permissive, but otherwise Quinn would need an extra Arc<Mutex<Session>> wrapper which would hurt performance.
pub trait Session: Clone + Sync + Send + Unpin + Sized + 'static {
    type SendStream: SendStream;
    type RecvStream: RecvStream;
    type Error: SessionError;

    /// Accept an incoming unidirectional stream
    fn poll_accept_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::RecvStream, Self::Error>>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    #[allow(clippy::type_complexity)]
    fn poll_accept_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new bidirectional stream.
    #[allow(clippy::type_complexity)]
    fn poll_open_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>>;

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::SendStream, Self::Error>>;

    /// Close the connection immediately
    fn close(&self, code: u32, reason: &[u8]);

    /// Check if the connection is closed, returing the error if it is.
    fn poll_closed(&self, cx: &mut Context<'_>) -> Poll<Self::Error>;

    /// A future that accepts an incoming unidirectional stream.
    fn accept_uni(&self) -> AcceptUni<Self> {
        AcceptUni {
            session: self.clone(),
        }
    }

    /// A future that accepts an incoming bidirectional stream.
    fn accept_bi(&self) -> AcceptBi<Self> {
        AcceptBi {
            session: self.clone(),
        }
    }

    /// A future that crates a new bidirectional stream.
    fn open_bi(&self) -> OpenBi<Self> {
        OpenBi {
            session: self.clone(),
        }
    }

    /// A future that crates a new unidirectional stream.
    fn open_uni(&self) -> OpenUni<Self> {
        OpenUni {
            session: self.clone(),
        }
    }

    /// A future that blocks until the connection is closed.
    fn closed(&self) -> Closed<Self> {
        Closed {
            session: self.clone(),
        }
    }
}

/// Trait that represent an error from the transport layer
pub trait SessionError: Error + Send + Sync + 'static {
    /// Get the QUIC error code from CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32>;
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct AcceptUni<S> {
    session: S,
}

impl<S> Future for AcceptUni<S>
where
    S: Session,
{
    type Output = Result<S::RecvStream, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_accept_uni(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct AcceptBi<S> {
    session: S,
}

impl<S> Future for AcceptBi<S>
where
    S: Session,
{
    type Output = Result<(S::SendStream, S::RecvStream), S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_accept_bi(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OpenUni<S> {
    session: S,
}

impl<S> Future for OpenUni<S>
where
    S: Session,
{
    type Output = Result<S::SendStream, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_open_uni(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OpenBi<S> {
    session: S,
}

impl<S> Future for OpenBi<S>
where
    S: Session,
{
    type Output = Result<(S::SendStream, S::RecvStream), S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_open_bi(cx)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Closed<S> {
    session: S,
}

impl<S> Future for Closed<S>
where
    S: Session,
{
    type Output = S::Error;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.session).poll_closed(cx)
    }
}
