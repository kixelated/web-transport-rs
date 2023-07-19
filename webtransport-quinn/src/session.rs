use std::{
    ops::{Deref, DerefMut},
    pin::pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::Future;

use crate::{Connect, RecvStream, SendStream, SessionError, Settings, WebTransportError};

use webtransport_proto::{Frame, StreamUni, VarInt};

/// An established WebTransport session, acting like a full QUIC connection. See [`quinn::Connection`].
///
/// It is important to remember that WebTransport is layered on top of QUIC:
///   1. Each stream starts with a few bytes identifying the stream type and session ID.
///   2. Errors codes are encoded with the session ID, so they aren't full QUIC error codes.
///   3. Stream IDs may have gaps in them, used by HTTP/3 transparant to the application.
///
/// Any non-overloaded methods can be using `Deref`. Be careful if you suspect a new Quinn method is not compatible with WebTransport and needs to be wrapped.
///
/// The session can be cloned so it can be accessed from multiple handles.
#[derive(Clone)]
pub struct Session {
    conn: quinn::Connection,
    session_id: VarInt,

    // Keep a reference to the settings and connect stream to avoid closing them.
    // We use Arc so the session can be cloned.
    #[allow(dead_code)]
    settings: Arc<Settings>,
    #[allow(dead_code)]
    connect: Arc<Connect>,

    // We also need to keep a reference to the qpack streams if the endpoint (incorrectly) creates them.
    // Again, this is just so they don't get closed until we drop the session.
    qpack_encoder: Arc<Mutex<Option<quinn::RecvStream>>>,
    qpack_decoder: Arc<Mutex<Option<quinn::RecvStream>>>,

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

        Self {
            conn,

            settings: Arc::new(settings),
            connect: Arc::new(connect),
            qpack_decoder: Arc::new(Mutex::new(None)),
            qpack_encoder: Arc::new(Mutex::new(None)),

            session_id,
            header_uni,
            header_bi,
        }
    }

    /// Open a new unidirectional stream. See [`quinn::Connection::open_uni`].
    pub async fn open_uni(&self) -> Result<SendStream, SessionError> {
        let mut send = self.conn.open_uni().await?;
        Self::write_full(&mut send, &self.header_uni).await?;
        Ok(SendStream::new(send))
    }

    /// Open a new bidirectional stream. See [`quinn::Connection::open_bi`].
    pub async fn open_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        let (mut send, recv) = self.conn.open_bi().await?;
        Self::write_full(&mut send, &self.header_bi).await?;
        Ok((SendStream::new(send), RecvStream::new(recv)))
    }

    /// Accept a new unidirectional stream. See [`quinn::Connection::accept_uni`].
    pub async fn accept_uni(&self) -> Result<RecvStream, SessionError> {
        loop {
            let mut recv = self.conn.accept_uni().await?;

            let typ = Self::read_varint(&mut recv).await?;
            let typ = StreamUni(typ);
            if typ.is_reserved() {
                // HTTP/3 reserved streams are ignored.
                continue;
            }

            match typ {
                StreamUni::WEBTRANSPORT => {
                    // Read the session ID and make sure it matches.
                    let session_id = Self::read_varint(&mut recv).await?;
                    if session_id != self.session_id {
                        return Err(WebTransportError::UnknownSession.into());
                    }

                    // Wrap the stream so we'll correct the error types.
                    let recv = RecvStream::new(recv);

                    return Ok(recv);
                }
                StreamUni::QPACK_ENCODER => {
                    // Save the qpack encoder stream so we will close it on drop.
                    // It's technically an error to send two of these but who cares, we'll close the previous one.
                    let mut qpack_encoder = self.qpack_encoder.lock().unwrap();
                    *qpack_encoder = Some(recv);
                }
                StreamUni::QPACK_DECODER => {
                    // Save the qpack encoder stream so we will close it on drop.
                    // It's technically an error to send two of these but who cares, we'll close the previous one.
                    let mut qpack_decoder = self.qpack_decoder.lock().unwrap();
                    *qpack_decoder = Some(recv);
                }

                // Who knows what this stream is for.
                // Close it, try another, and hope the other endpoint doesn't close the session.
                _ => (),
            }
        }
    }

    /// Accept a new bidirectional stream. See [`quinn::Connection::accept_bi`].
    pub async fn accept_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        loop {
            let (send, mut recv) = self.conn.accept_bi().await?;

            let typ = Self::read_varint(&mut recv).await?;
            if Frame(typ) != Frame::WEBTRANSPORT {
                // Close it, accept the next, and hope we don't error.
                // TODO write a Method not Allowed HTTP/3 response?
                continue;
            }

            let session_id = Self::read_varint(&mut recv).await?;
            if session_id != self.session_id {
                return Err(WebTransportError::UnknownSession.into());
            }

            return Ok((SendStream::new(send), RecvStream::new(recv)));
        }
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
        let mut cursor = std::io::Cursor::new(&buf[..size]);
        let v = VarInt::decode(&mut cursor).unwrap();

        Ok(v)
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

impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

impl webtransport_generic::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = SessionError;

    /// Accept an incoming unidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::RecvStream, Self::Error>> {
        pin!(self.accept_uni()).poll(cx)
    }

    /// Accept an incoming bidirectional stream
    ///
    /// Returning `None` implies the connection is closing or closed.
    fn poll_accept_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        pin!(self.accept_bi()).poll(cx)
    }

    /// Poll the connection to create a new bidirectional stream.
    fn poll_open_bidi(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        pin!(self.open_bi()).poll(cx)
    }

    /// Poll the connection to create a new unidirectional stream.
    fn poll_open_uni(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>> {
        pin!(self.open_uni()).poll(cx)
    }

    /// Close the connection immediately
    fn close(&mut self, code: u32, reason: &[u8]) {
        Session::close(self, code, reason)
    }
}
