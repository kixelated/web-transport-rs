use std::{
    fmt,
    future::{poll_fn, Future},
    io::Cursor,
    ops::Deref,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{ready, Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures::stream::{FuturesUnordered, Stream, StreamExt};
use tokio::io::AsyncReadExt;
use url::Url;

use crate::{
    ClientError, Connect, RecvStream, SendStream, SessionError, Settings, WebTransportError,
};

use web_transport_proto::{Frame, StreamUni, VarInt};

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

    // The session ID, as determined by the stream ID of the connect request.
    session_id: Option<VarInt>,

    // The accept logic is stateful, so use an Arc<Mutex> to share it.
    accept: Option<Arc<Mutex<SessionAccept>>>,

    // Cache the headers in front of each stream we open.
    header_uni: Vec<u8>,
    header_bi: Vec<u8>,
    header_datagram: Vec<u8>,

    // Keep a reference to the settings and connect stream to avoid closing them until dropped.
    #[allow(dead_code)]
    settings: Option<Arc<Settings>>,

    // The URL used to create the session.
    url: Url,
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

        let mut header_datagram = Vec::new();
        session_id.encode(&mut header_datagram);

        // Accept logic is stateful, so use an Arc<Mutex> to share it.
        let accept = SessionAccept::new(conn.clone(), session_id);

        let this = Self {
            conn,
            accept: Some(Arc::new(Mutex::new(accept))),
            session_id: Some(session_id),
            header_uni,
            header_bi,
            header_datagram,
            url: connect.url().clone(),
            settings: Some(Arc::new(settings)),
        };

        // Run a background task to check if the connect stream is closed.
        let mut this2 = this.clone();
        tokio::spawn(async move {
            let (code, reason) = this2.run_closed(connect).await;
            this2.close(code, reason.as_bytes());
        });

        this
    }

    // Keep reading from the control stream until it's closed.
    async fn run_closed(&mut self, connect: Connect) -> (u32, String) {
        let (_send, mut recv) = connect.into_inner();

        let mut buf = Vec::new();

        loop {
            // Keep reading from the stream until we get a closed capsule.
            match recv.read_buf(&mut buf).await {
                Ok(0) => return (0, "".to_string()),
                Ok(_) => {}
                // std::io::Error is pretty useless
                Err(_err) => return (1, "read error".to_string()),
            };

            let mut cursor = Cursor::new(&buf);

            match web_transport_proto::Capsule::decode(&mut cursor) {
                Ok(capsule) => match capsule {
                    web_transport_proto::Capsule::CloseWebTransportSession { code, reason } => {
                        return (code, reason)
                    }
                    web_transport_proto::Capsule::Unknown { typ, payload } => {
                        log::warn!("unknown capsule: type={typ} size={}", payload.len());
                    }
                },
                Err(web_transport_proto::CapsuleError::UnexpectedEnd) => continue, // More data needed.
                Err(err) => {
                    log::warn!("control stream capsule error: {err:?}");
                    return (1, "capsule error".to_string());
                }
            };

            buf.drain(..cursor.position() as usize);
        }
    }

    /// Connect using an established QUIC connection if you want to create the connection yourself.
    /// This will only work with a brand new QUIC connection using the HTTP/3 ALPN.
    pub async fn connect(conn: quinn::Connection, url: Url) -> Result<Session, ClientError> {
        // Perform the H3 handshake by sending/reciving SETTINGS frames.
        let settings = Settings::connect(&conn).await?;

        // Send the HTTP/3 CONNECT request.
        let connect = Connect::open(&conn, url).await?;

        // Return the resulting session with a reference to the control/connect streams.
        // If either stream is closed, then the session will be closed, so we need to keep them around.
        let session = Session::new(conn, settings, connect);

        Ok(session)
    }

    /// Accept a new unidirectional stream. See [`quinn::Connection::accept_uni`].
    pub async fn accept_uni(&self) -> Result<RecvStream, SessionError> {
        if let Some(accept) = &self.accept {
            poll_fn(|cx| accept.lock().unwrap().poll_accept_uni(cx)).await
        } else {
            self.conn
                .accept_uni()
                .await
                .map(|recv| RecvStream::new(Arc::new(recv.id()), recv))
                .map_err(Into::into)
        }
    }

    /// Accept a new bidirectional stream. See [`quinn::Connection::accept_bi`].
    pub async fn accept_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        if let Some(accept) = &self.accept {
            poll_fn(|cx| accept.lock().unwrap().poll_accept_bi(cx)).await
        } else {
            self.conn
                .accept_bi()
                .await
                .map(|(send, recv)| {
                    debug_assert_eq!(send.id(), recv.id());
                    let id = Arc::new(send.id());
                    (
                        SendStream::new(Arc::clone(&id), send),
                        RecvStream::new(id, recv),
                    )
                })
                .map_err(Into::into)
        }
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
        Ok(SendStream::new(Arc::new(send.id()), send))
    }

    /// Open a new bidirectional stream. See [`quinn::Connection::open_bi`].
    pub async fn open_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        let (mut send, recv) = self.conn.open_bi().await?;

        // Set the stream priority to max and then write the stream header.
        // Otherwise the application could write data with lower priority than the header, resulting in queuing.
        // Also the header is very important for determining the session ID without reliable reset.
        send.set_priority(i32::MAX).ok();
        Self::write_full(&mut send, &self.header_bi).await?;

        debug_assert_eq!(send.id(), recv.id());
        let id = Arc::new(send.id());

        // Reset the stream priority back to the default of 0.
        send.set_priority(0).ok();
        Ok((
            SendStream::new(Arc::clone(&id), send),
            RecvStream::new(id, recv),
        ))
    }

    /// Asynchronously receives an application datagram from the remote peer.
    ///
    /// This method is used to receive an application datagram sent by the remote
    /// peer over the connection.
    /// It waits for a datagram to become available and returns the received bytes.
    pub async fn read_datagram(&self) -> Result<Bytes, SessionError> {
        let mut datagram = self.conn.read_datagram().await?;

        let mut cursor = Cursor::new(&datagram);

        if let Some(session_id) = self.session_id {
            // We have to check and strip the session ID from the datagram.
            let actual_id = VarInt::decode(&mut cursor).map_err(|_| {
                WebTransportError::ReadError(quinn::ReadExactError::FinishedEarly(0))
            })?;
            if actual_id != session_id {
                return Err(WebTransportError::UnknownSession.into());
            }
        }

        // Return the datagram without the session ID.
        let datagram = datagram.split_off(cursor.position() as usize);

        Ok(datagram)
    }

    /// Sends an application datagram to the remote peer.
    ///
    /// Datagrams are unreliable and may be dropped or delivered out of order.
    /// The data must be smaller than [`max_datagram_size`](Self::max_datagram_size).
    pub fn send_datagram(&self, data: Bytes) -> Result<(), SessionError> {
        if !self.header_datagram.is_empty() {
            // Unfortunately, we need to allocate/copy each datagram because of the Quinn API.
            // Pls go +1 if you care: https://github.com/quinn-rs/quinn/issues/1724
            let mut buf = BytesMut::with_capacity(self.header_datagram.len() + data.len());

            // Prepend the datagram with the header indicating the session ID.
            buf.extend_from_slice(&self.header_datagram);
            buf.extend_from_slice(&data);

            self.conn.send_datagram(buf.into())?;
        } else {
            self.conn.send_datagram(data)?;
        }

        Ok(())
    }

    /// Computes the maximum size of datagrams that may be passed to
    /// [`send_datagram`](Self::send_datagram).
    pub fn max_datagram_size(&self) -> usize {
        let mtu = self
            .conn
            .max_datagram_size()
            .expect("datagram support is required");
        mtu.saturating_sub(self.header_datagram.len())
    }

    /// Immediately close the connection with an error code and reason. See [`quinn::Connection::close`].
    pub fn close(&self, code: u32, reason: &[u8]) {
        let code = if self.session_id.is_some() {
            web_transport_proto::error_to_http3(code)
                .try_into()
                .unwrap()
        } else {
            code.into()
        };

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

    /// Create a new session from a raw QUIC connection and a URL.
    ///
    /// This is used to pretend like a QUIC connection is a WebTransport session.
    /// It's a hack, but it makes it much easier to support WebTransport and raw QUIC simultaneously.
    pub fn raw(conn: quinn::Connection, url: Url) -> Self {
        Self {
            conn,
            session_id: None,
            header_uni: Default::default(),
            header_bi: Default::default(),
            header_datagram: Default::default(),
            accept: None,
            settings: None,
            url,
        }
    }

    pub fn url(&self) -> &Url {
        &self.url
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

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.conn.stable_id() == other.conn.stable_id()
    }
}

impl Eq for Session {}

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
    pub(crate) fn new(conn: quinn::Connection, session_id: VarInt) -> Self {
        // Create a stream that just outputs new streams, so it's easy to call from poll.
        let accept_uni = Box::pin(futures::stream::unfold(conn.clone(), |conn| async {
            Some((conn.accept_uni().await, conn))
        }));

        let accept_bi = Box::pin(futures::stream::unfold(conn, |conn| async {
            Some((conn.accept_bi().await, conn))
        }));

        Self {
            session_id,

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
                    let recv = RecvStream::new(Arc::new(recv.id()), recv);
                    return Poll::Ready(Ok(recv));
                }
                StreamUni::QPACK_DECODER => {
                    self.qpack_decoder = Some(recv);
                }
                StreamUni::QPACK_ENCODER => {
                    self.qpack_encoder = Some(recv);
                }
                _ => {
                    // ignore unknown streams
                    log::debug!("ignoring unknown unidirectional stream: {typ:?}");
                }
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
                debug_assert_eq!(send.id(), recv.id());
                let id = Arc::new(send.id());
                // Wrap the streams in our own types for correct error codes.
                let send = SendStream::new(Arc::clone(&id), send);
                let recv = RecvStream::new(id, recv);
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
            log::debug!("ignoring unknown bidirectional stream: {typ:?}");
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

impl web_transport_trait::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = SessionError;

    async fn accept_uni(&self) -> Result<Self::RecvStream, Self::Error> {
        Self::accept_uni(self).await
    }

    async fn accept_bi(&self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        Self::accept_bi(self).await
    }

    async fn open_bi(&self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        Self::open_bi(self).await
    }

    async fn open_uni(&self) -> Result<Self::SendStream, Self::Error> {
        Self::open_uni(self).await
    }

    fn close(&self, code: u32, reason: &str) {
        Self::close(self, code, reason.as_bytes());
    }

    async fn closed(&self) -> Self::Error {
        Self::closed(self).await
    }

    fn send_datagram(&self, data: Bytes) -> Result<(), Self::Error> {
        Self::send_datagram(self, data)
    }

    async fn recv_datagram(&self) -> Result<Bytes, Self::Error> {
        Self::read_datagram(self).await
    }

    fn max_datagram_size(&self) -> usize {
        Self::max_datagram_size(self)
    }
}
