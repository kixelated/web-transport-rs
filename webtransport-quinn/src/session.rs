use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use thiserror::Error;

use crate::{Connect, RecvStream, SendStream, Settings};

use webtransport_proto::{Frame, StreamUni, VarInt};

/// An established WebTransport session, acting like a full QUIC connection.
/// This is a thin wrapper around [`quinn::Connection`] using `Deref` to access any methods that are not overloaded.
///
/// It is important to remember that WebTransport is layered on top of QUIC:
///   1. Each stream starts with a few bytes identifying the stream type and session ID.
///   2. Errors codes are encoded with the session ID, so they aren't full QUIC error codes.
///   3. Stream IDs may have gaps in them, used by HTTP/3 transparant to the application.
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

    // Cache the headers in front of each stream we open.
    header_uni: Vec<u8>,
    header_bi: Vec<u8>,
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("connection error")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("write error")]
    WriteError(#[from] quinn::WriteError),

    #[error("read error")]
    ReadError(#[from] quinn::ReadError),

    #[error("unexpected end of stream")]
    UnexpectedEnd,
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

            session_id,
            header_uni,
            header_bi,
        }
    }

    /// Open a new unidirectional stream. See [`quinn::Connection::open_uni`].
    pub async fn open_uni(&self) -> Result<SendStream, SessionError> {
        let mut send = self.conn.open_uni().await?;
        send.write_all(&self.header_uni).await?;
        Ok(SendStream::new(send))
    }

    /// Open a new bidirectional stream. See [`quinn::Connection::open_bi`].
    pub async fn open_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        let (mut send, recv) = self.conn.open_bi().await?;
        send.write_all(&self.header_bi).await?;
        Ok((SendStream::new(send), RecvStream::new(recv)))
    }

    /// Accept a new unidirectional stream. See [`quinn::Connection::accept_uni`].
    pub async fn accept_uni(&self) -> Result<RecvStream, SessionError> {
        loop {
            let mut recv = self.conn.accept_uni().await?;

            let typ = StreamUni(read_varint(&mut recv).await?);
            if typ.is_reserved() {
                // HTTP/3 reserved streams are ignored.
                continue;
            }

            if typ != StreamUni::WEBTRANSPORT {
                // TODO just keep looping.
                return Err(quinn::ReadError::UnknownStream.into());
            }

            let session_id = read_varint(&mut recv).await?;
            if session_id != self.session_id {
                // TODO return a better error message: unknown session
                return Err(quinn::ReadError::UnknownStream.into());
            }

            return Ok(RecvStream::new(recv));
        }
    }

    /// Accept a new bidirectional stream. See [`quinn::Connection::accept_bi`].
    pub async fn accept_bi(&self) -> Result<(SendStream, RecvStream), SessionError> {
        let (send, mut recv) = self.conn.accept_bi().await?;

        let typ = Frame(read_varint(&mut recv).await?);
        if typ != Frame::WEBTRANSPORT {
            return Err(quinn::ReadError::UnknownStream.into());
        }

        let session_id = read_varint(&mut recv).await?;
        if session_id != self.session_id {
            // TODO return a better error message: unknown session
            return Err(quinn::ReadError::UnknownStream.into());
        }

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

    pub fn close(&self) {
        unimplemented!("close")
    }

    pub fn close_reason(&self) {
        unimplemented!("close")
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

// Read a varint from the stream.
async fn read_varint(stream: &mut quinn::RecvStream) -> Result<VarInt, SessionError> {
    // 8 bytes is the max size of a varint
    let mut buf = [0; 8];

    // Read the first byte because it includes the length.
    match stream.read_exact(&mut buf[0..1]).await {
        Ok(()) => (),
        Err(quinn::ReadExactError::FinishedEarly) => return Err(SessionError::UnexpectedEnd),
        Err(quinn::ReadExactError::ReadError(e)) => return Err(e.into()),
    };

    // 0b00 = 1, 0b01 = 2, 0b10 = 4, 0b11 = 8
    let size = 1 << (buf[0] >> 6);
    match stream.read_exact(&mut buf[1..size]).await {
        Ok(()) => (),
        Err(quinn::ReadExactError::FinishedEarly) => return Err(SessionError::UnexpectedEnd),
        Err(quinn::ReadExactError::ReadError(e)) => return Err(e.into()),
    };

    // Use a cursor to read the varint on the stack.
    let mut cursor = std::io::Cursor::new(&buf[..size]);
    let v = VarInt::decode(&mut cursor).unwrap();

    Ok(v)
}
