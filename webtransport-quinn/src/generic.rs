use bytes::{Buf, Bytes};
use futures::{
    ready,
    stream::{self, BoxStream},
    StreamExt,
};
use quinn::VarInt;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio_util::sync::ReusableBoxFuture;

pub struct SendStream {
    stream: Option<crate::SendStream>,
    write_fut: WriteFuture,
}

type WriteFuture =
    ReusableBoxFuture<'static, (crate::SendStream, Result<usize, quinn::WriteError>)>;

impl SendStream {
    fn new(stream: crate::SendStream) -> SendStream {
        Self {
            stream: Some(stream),
            write_fut: ReusableBoxFuture::new(async { unreachable!() }),
        }
    }
}

impl webtransport_generic::SendStream for SendStream {
    type Error = quinn::WriteError;

    fn poll_send<B: Buf>(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<Result<usize, Self::Error>> {
        let s = Pin::new(self.stream.as_mut().unwrap());

        let res = ready!(futures::io::AsyncWrite::poll_write(s, cx, buf.chunk()));
        match res {
            Ok(written) => {
                buf.advance(written);
                Poll::Ready(Ok(written))
            }
            Err(err) => {
                // We are forced to use AsyncWrite for now because we cannot store
                // the result of a call to:
                // quinn::send_stream::write<'a>(&'a mut self, buf: &'a [u8]) -> Result<usize, WriteError>.
                //
                // This is why we have to unpack the error from io::Error instead of having it
                // returned directly. This should not panic as long as quinn's AsyncWrite impl
                // doesn't change.
                let err = err
                    .into_inner()
                    .expect("write stream returned an empty error")
                    .downcast::<quinn::WriteError>()
                    .expect("write stream returned an error which type is not WriteError");

                Poll::Ready(Err(*err))
            }
        }
    }

    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.stream
            .as_mut()
            .unwrap()
            .poll_finish(cx)
            .map_err(Into::into)
    }

    fn reset(&mut self, reset_code: u32) {
        let _ = self
            .stream
            .as_mut()
            .unwrap()
            .reset(VarInt::from_u32(reset_code));
    }
}

struct RecvStream {
    stream: Option<crate::RecvStream>,
    read_fut: ReadFuture,
}

type ReadFuture = ReusableBoxFuture<
    'static,
    (
        crate::RecvStream,
        Result<Option<quinn::Chunk>, quinn::ReadError>,
    ),
>;

impl RecvStream {
    fn new(stream: crate::RecvStream) -> Self {
        Self {
            stream: Some(stream),
            // Should only allocate once the first time it's used
            read_fut: ReusableBoxFuture::new(async { unreachable!() }),
        }
    }
}

impl webtransport_generic::RecvStream for RecvStream {
    /// The type of `Buf` for data received on this stream.
    type Buf = Bytes;
    /// The error type that can occur when receiving data.
    type Error = quinn::ReadError;

    /// Poll the stream for more data.
    ///
    /// When the receive side will no longer receive more data (such as because
    /// the peer closed their sending side), this should return `None`.
    fn poll_data(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Self::Buf>, Self::Error>> {
        if let Some(mut stream) = self.stream.take() {
            self.read_fut.set(async move {
                let chunk = stream.read_chunk(usize::MAX, true).await;
                (stream, chunk)
            })
        };

        let (stream, chunk) = ready!(self.read_fut.poll(cx));
        self.stream = Some(stream);

        Poll::Ready(Ok(chunk?.map(|c| c.bytes)))
    }

    /// Send a `STOP_SENDING` QUIC code.
    fn stop_sending(&mut self, error_code: u32) {
        self.stream
            .as_mut()
            .unwrap()
            .stop(quinn::VarInt::from_u32(error_code))
            .ok();
    }
}

/*
struct BidiStream(SendStream, RecvStream);

impl webtransport_generic::BidiStream for BidiStream {
    /// The type for the send half.
    type SendStream = SendStream;
    /// The type for the receive half.
    type RecvStream = RecvStream;

    /// Split this stream into two halves.
    fn split(self) -> (Self::SendStream, Self::RecvStream) {
        (self.0, self.1)
    }
}
*/

pub struct Session {
    session: crate::Session,
    incoming_bi:
        BoxStream<'static, Result<(crate::SendStream, crate::RecvStream), crate::SessionError>>,
    opening_bi: Option<
        BoxStream<'static, Result<(crate::SendStream, crate::RecvStream), crate::SessionError>>,
    >,
    incoming_uni: BoxStream<'static, Result<crate::RecvStream, crate::SessionError>>,
    opening_uni: Option<BoxStream<'static, Result<crate::SendStream, crate::SessionError>>>,
}

impl Session {
    fn new(session: crate::Session) -> Self {
        Self {
            session,
            incoming_bi: Box::pin(stream::unfold(session.clone(), |session| async {
                Some((session.accept_bi().await, session))
            })),
            opening_bi: None,
            incoming_uni: Box::pin(stream::unfold(session.clone(), |session| async {
                Some((session.accept_uni().await, session))
            })),
            opening_uni: None,
        }
    }
}

impl webtransport_generic::Connection for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    //type BidiStream = BidiStream;
    type Error = crate::SessionError;

    /// Accept an incoming unidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::RecvStream>, Self::Error>> {
        let recv = match ready!(self.incoming_uni.poll_next_unpin(cx)) {
            Some(x) => x?,
            None => return Poll::Ready(Ok(None)),
        };
        Poll::Ready(Ok(Some(Self::RecvStream::new(recv))))
    }

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<(Self::SendStream, Self::RecvStream)>, Self::Error>> {
        let (send, recv) = match ready!(self.incoming_bi.poll_next_unpin(cx)) {
            Some(x) => x?,
            None => return Poll::Ready(Ok(None)),
        };
        Poll::Ready(Ok(Some((
            Self::SendStream::new(send),
            Self::RecvStream::new(recv),
        ))))
    }

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        if self.opening_bi.is_none() {
            self.opening_bi = Some(Box::pin(stream::unfold(
                self.session.clone(),
                |session| async { Some((session.clone().open_bi().await, session)) },
            )));
        }

        let (send, recv) =
            ready!(self.opening_bi.as_mut().unwrap().poll_next_unpin(cx)).unwrap()?;
        Poll::Ready(Ok((
            Self::SendStream::new(send),
            Self::RecvStream::new(recv),
        )))
    }

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>> {
        if self.opening_uni.is_none() {
            self.opening_uni = Some(Box::pin(stream::unfold(
                self.session.clone(),
                |session| async { Some((session.open_uni().await, session)) },
            )));
        }

        let send = ready!(self.opening_uni.as_mut().unwrap().poll_next_unpin(cx)).unwrap()?;
        Poll::Ready(Ok(Self::SendStream::new(send)))
    }

    /// Close the connection immediately
    fn close(&mut self, code: u32, reason: &[u8]) {}
}
