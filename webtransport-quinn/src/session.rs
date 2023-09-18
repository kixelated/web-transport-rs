use std::{
    fmt,
    future::{poll_fn, Future},
    io::Cursor,
    ops::Deref,
    pin::{pin, Pin},
    sync::{Arc, Mutex},
    task::{ready, Context, Poll},
};

use futures::stream::{FuturesUnordered, Stream, StreamExt};

use crate::{Connect, RecvStream, SendStream, SessionError, Settings, WebTransportError};

use webtransport_proto::{Frame, StreamUni, VarInt};

/// An established WebTransport session, acting like a full QUIC connection. See [`quinn::Connection`].
///
/// It is important to remember that WebTransport is layered on top of QUIC:
///   1. Each stream starts with a few bytes identifying the stream type and session ID.
///   2. Errors codes are encoded with the session ID, so they aren't full QUIC error codes.
///   3. Stream IDs may have gaps in them, used by HTTP/3 transparant to the application.
///
/// Deref is used to expose non-overloaded methods on [`quinn::Connection`].
/// These should be safe to use with WebTransport, but file a PR if you find one that isn't.
#[derive(Clone)]
pub struct Session {
    conn: quinn::Connection,

    // The accept logic is stateful, so use an Arc<Mutex> to share it.
    accept: Arc<Mutex<SessionAccept>>,

    // Cache the headers in front of each stream we open.
    header_uni: Vec<u8>,
    header_bi: Vec<u8>,
}

impl Session {
    pub(crate) fn new(conn: quinn::Connection, settings: Settings, connect: Connect) -> Self {
        // The session ID is the stream ID of the CONNECT request.
        let session_id = connect.session_id();

        // Cache the tiny header we write in front of each stream we open.
        let mut header_uni = Vec::new();
        StreamUni::WEBTRANSPORT.encode(&mut header_uni);
        session_id.encode(&mut header_uni);

        let mut header_bi = Vec::new();
        Frame::WEBTRANSPORT.encode(&mut header_bi);
        session_id.encode(&mut header_bi);

        // Accept logic is stateful, so use an Arc<Mutex> to share it.
        let accept = SessionAccept::new(conn.clone(), settings, connect);

        Self {
            conn,
            accept: Arc::new(Mutex::new(accept)),
            header_uni,
            header_bi,
        }
    }

    /// Accept a new unidirectional stream. See [`quinn::Connection::accept_uni`].
    pub async fn accept_uni(&self) -> Result<RecvStream, SessionError> {
        poll_fn(|cx| self.accept.lock().unwrap().poll_accept_uni(cx)).await
    }

    /// Accept a new bidirectional stream. See [`quinn::Connection::accept_bi`].
    pub async fn accept_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        poll_fn(|cx| self.accept.lock().unwrap().poll_accept_bi(cx)).await
    }

    /// Open a new unidirectional stream. See [`quinn::Connection::open_uni`].
    pub async fn open_uni(&self) -> Result<SendStream, SessionError> {
        let mut send = self.conn.open_uni().await?;

        // Set the stream priority to max and then write the stream header.
        // Otherwise the application could write data with lower priority than the header, resulting in queuing.
        // Also the header is very important for determining the session ID without reliable reset.
        send.set_priority(i32::MAX).ok();
        Self::write_full(&mut send, &self.header_uni).await?;

        // Reset the stream priority back to the default of 0.
        send.set_priority(0).ok();
        Ok(SendStream::new(send))
    }

    /// Open a new bidirectional stream. See [`quinn::Connection::open_bi`].
    pub async fn open_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        let (mut send, recv) = self.conn.open_bi().await?;

        // Set the stream priority to max and then write the stream header.
        // Otherwise the application could write data with lower priority than the header, resulting in queuing.
        // Also the header is very important for determining the session ID without reliable reset.
        send.set_priority(i32::MAX).ok();
        Self::write_full(&mut send, &self.header_bi).await?;

        // Reset the stream priority back to the default of 0.
        send.set_priority(0).ok();
        Ok((SendStream::new(send), RecvStream::new(recv)))
    }

    pub async fn read_datagram(&self) {
        unimplemented!("datagrams")
    }

    pub async fn send_datagram(&self) {
        unimplemented!("datagrams")
    }

    pub fn max_datagram_size(&self) {
        unimplemented!("datagrams")
    }

    /// Immediately close the connection with an error code and reason. See [`quinn::Connection::close`].
    pub fn close(&self, code: u32, reason: &[u8]) {
        let code = webtransport_proto::error_to_http3(code).try_into().unwrap();
        self.conn.close(code, reason)
    }

    /// Wait until the session is closed, returning the error. See [`quinn::Connection::closed`].
    pub async fn closed(&self) -> SessionError {
        self.conn.closed().await.into()
    }

    /// Return why the session was closed, or None if it's not closed. See [`quinn::Connection::close_reason`].
    pub fn close_reason(&self) -> Option<SessionError> {
        self.conn.close_reason().map(Into::into)
    }

    async fn write_full(send: &mut quinn::SendStream, buf: &[u8]) -> Result<(), SessionError> {
        match send.write_all(buf).await {
            Ok(_) => Ok(()),
            Err(quinn::WriteError::ConnectionLost(err)) => Err(err.into()),
            Err(err) => Err(WebTransportError::WriteError(err).into()),
        }
    }
}

impl Deref for Session {
    type Target = quinn::Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.conn.fmt(f)
    }
}

// Type aliases just so clippy doesn't complain about the complexity.
type AcceptUni = dyn Stream<Item = Result<quinn::RecvStream, quinn::ConnectionError>> + Send;
type AcceptBi = dyn Stream<Item = Result<(quinn::SendStream, quinn::RecvStream), quinn::ConnectionError>>
    + Send;
type PendingUni = dyn Future<Output = Result<(StreamUni, quinn::RecvStream), SessionError>> + Send;
type PendingBi = dyn Future<Output = Result<Option<(quinn::SendStream, quinn::RecvStream)>, SessionError>>
    + Send;

// Logic just for accepting streams, which is annoying because of the stream header.
pub struct SessionAccept {
    session_id: VarInt,

    // Keep a reference to the settings and connect stream to avoid closing them until dropped.
    #[allow(dead_code)]
    settings: Settings,
    #[allow(dead_code)]
    connect: Connect,

    // We also need to keep a reference to the qpack streams if the endpoint (incorrectly) creates them.
    // Again, this is just so they don't get closed until we drop the session.
    qpack_encoder: Option<quinn::RecvStream>,
    qpack_decoder: Option<quinn::RecvStream>,

    accept_uni: Pin<Box<AcceptUni>>,
    accept_bi: Pin<Box<AcceptBi>>,

    // Keep track of work being done to read/write the WebTransport stream header.
    pending_uni: FuturesUnordered<Pin<Box<PendingUni>>>,
    pending_bi: FuturesUnordered<Pin<Box<PendingBi>>>,
}

impl SessionAccept {
    pub(crate) fn new(conn: quinn::Connection, settings: Settings, connect: Connect) -> Self {
        // The session ID is the stream ID of the CONNECT request.
        let session_id = connect.session_id();

        // Create a stream that just outputs new streams, so it's easy to call from poll.
        let accept_uni = Box::pin(futures::stream::unfold(conn.clone(), |conn| async {
            Some((conn.accept_uni().await, conn))
        }));

        let accept_bi = Box::pin(futures::stream::unfold(conn, |conn| async {
            Some((conn.accept_bi().await, conn))
        }));

        Self {
            session_id,

            settings,
            connect,
            qpack_decoder: None,
            qpack_encoder: None,

            accept_uni,
            accept_bi,

            pending_uni: FuturesUnordered::new(),
            pending_bi: FuturesUnordered::new(),
        }
    }

    // This is poll-based because we accept and decode streams in parallel.
    // In async land I would use tokio::JoinSet, but that requires a runtime.
    // It's better to use FuturesUnordered instead because it's agnostic.
    pub fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<RecvStream, SessionError>> {
        loop {
            // Accept any new streams.
            if let Poll::Ready(Some(res)) = self.accept_uni.poll_next_unpin(cx) {
                // Start decoding the header and add the future to the list of pending streams.
                let recv = res?;
                let pending = Self::decode_uni(recv, self.session_id);
                self.pending_uni.push(Box::pin(pending));

                continue;
            }

            // Poll the list of pending streams.
            let (typ, recv) = match ready!(self.pending_uni.poll_next_unpin(cx)) {
                Some(res) => res?,
                None => return Poll::Pending,
            };

            // Decide if we keep looping based on the type.
            match typ {
                StreamUni::WEBTRANSPORT => {
                    let recv = RecvStream::new(recv);
                    return Poll::Ready(Ok(recv));
                }
                StreamUni::QPACK_DECODER => {
                    self.qpack_decoder = Some(recv);
                }
                StreamUni::QPACK_ENCODER => {
                    self.qpack_encoder = Some(recv);
                }
                _ => {} // ignore unknown streams
            }
        }
    }

    // Reads the stream header, returning the stream type.
    async fn decode_uni(
        mut recv: quinn::RecvStream,
        expected_session: VarInt,
    ) -> Result<(StreamUni, quinn::RecvStream), SessionError> {
        // Read the VarInt at the start of the stream.
        let typ = Self::read_varint(&mut recv).await?;
        let typ = StreamUni(typ);

        if typ == StreamUni::WEBTRANSPORT {
            // Read the session_id and validate it
            let session_id = Self::read_varint(&mut recv).await?;
            if session_id != expected_session {
                return Err(WebTransportError::UnknownSession.into());
            }
        }

        // We need to keep a reference to the qpack streams if the endpoint (incorrectly) creates them, so return everything.
        Ok((typ, recv))
    }

    pub fn poll_accept_bi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(SendStream, RecvStream), SessionError>> {
        loop {
            // Accept any new streams.
            if let Poll::Ready(Some(res)) = self.accept_bi.poll_next_unpin(cx) {
                // Start decoding the header and add the future to the list of pending streams.
                let (send, recv) = res?;
                let pending = Self::decode_bi(send, recv, self.session_id);
                self.pending_bi.push(Box::pin(pending));

                continue;
            }

            // Poll the list of pending streams.
            let res = match ready!(self.pending_bi.poll_next_unpin(cx)) {
                Some(res) => res?,
                None => return Poll::Pending,
            };

            if let Some((send, recv)) = res {
                // Wrap the streams in our own types for correct error codes.
                let send = SendStream::new(send);
                let recv = RecvStream::new(recv);
                return Poll::Ready(Ok((send, recv)));
            }

            // Keep looping if it's a stream we want to ignore.
        }
    }

    // Reads the stream header, returning Some if it's a WebTransport stream.
    async fn decode_bi(
        send: quinn::SendStream,
        mut recv: quinn::RecvStream,
        expected_session: VarInt,
    ) -> Result<Option<(quinn::SendStream, quinn::RecvStream)>, SessionError> {
        let typ = Self::read_varint(&mut recv).await?;
        if Frame(typ) != Frame::WEBTRANSPORT {
            return Ok(None);
        }

        // Read the session ID and validate it.
        let session_id = Self::read_varint(&mut recv).await?;
        if session_id != expected_session {
            return Err(WebTransportError::UnknownSession.into());
        }

        Ok(Some((send, recv)))
    }

    // Read into the provided buffer and cast any errors to SessionError.
    async fn read_full(recv: &mut quinn::RecvStream, buf: &mut [u8]) -> Result<(), SessionError> {
        match recv.read_exact(buf).await {
            Ok(()) => Ok(()),
            Err(quinn::ReadExactError::ReadError(quinn::ReadError::ConnectionLost(err))) => {
                Err(err.into())
            }
            Err(err) => Err(WebTransportError::ReadError(err).into()),
        }
    }

    // Read a varint from the stream.
    async fn read_varint(recv: &mut quinn::RecvStream) -> Result<VarInt, SessionError> {
        // 8 bytes is the max size of a varint
        let mut buf = [0; 8];

        // Read the first byte because it includes the length.
        Self::read_full(recv, &mut buf[0..1]).await?;

        // 0b00 = 1, 0b01 = 2, 0b10 = 4, 0b11 = 8
        let size = 1 << (buf[0] >> 6);
        Self::read_full(recv, &mut buf[1..size]).await?;

        // Use a cursor to read the varint on the stack.
        let mut cursor = Cursor::new(&buf[..size]);
        let v = VarInt::decode(&mut cursor).unwrap();

        Ok(v)
    }
}

impl webtransport_generic::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = SessionError;

    /// Accept an incoming unidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::RecvStream, Self::Error>> {
        self.accept.lock().unwrap().poll_accept_uni(cx)
    }

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        self.accept.lock().unwrap().poll_accept_bi(cx)
    }

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bi(
        &self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        pin!(self.open_bi()).poll(cx)
    }

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(&self, cx: &mut Context<'_>) -> Poll<Result<Self::SendStream, Self::Error>> {
        pin!(self.open_uni()).poll(cx)
    }

    /// Close the connection immediately
    fn close(&self, code: u32, reason: &[u8]) {
        Session::close(self, code, reason)
    }

    fn poll_closed(&self, cx: &mut Context<'_>) -> Poll<Self::Error> {
        pin!(self.closed()).poll(cx)
    }
}
