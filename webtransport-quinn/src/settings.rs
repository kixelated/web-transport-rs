use futures::try_join;
use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SettingsError {
    #[error("quic stream was closed early")]
    UnexpectedEnd,

    #[error("protocol error: {0}")]
    ProtoError(#[from] webtransport_proto::SettingsError),

    #[error("WebTransport is not supported")]
    WebTransportUnsupported,

    #[error("connection error")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("read error")]
    ReadError(#[from] quinn::ReadError),

    #[error("write error")]
    WriteError(#[from] quinn::WriteError),
}

pub struct Settings {
    // A reference to the send/recv stream, so we don't close it until dropped.
    #[allow(dead_code)]
    send: quinn::SendStream,

    #[allow(dead_code)]
    recv: quinn::RecvStream,
}

impl Settings {
    // Establish the H3 connection.
    pub async fn connect(conn: &quinn::Connection) -> Result<Self, SettingsError> {
        let recv = Self::accept(conn);
        let send = Self::open(conn);

        // Run both tasks concurrently until one errors or they both complete.
        let (send, recv) = try_join!(send, recv)?;
        Ok(Self { send, recv })
    }

    async fn accept(conn: &quinn::Connection) -> Result<quinn::RecvStream, SettingsError> {
        let mut recv = conn.accept_uni().await?;
        let mut buf = Vec::new();

        loop {
            // Read more data into the buffer.
            let chunk = recv.read_chunk(usize::MAX, true).await?;
            let chunk = chunk.ok_or(SettingsError::UnexpectedEnd)?;
            buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

            // Look at the buffer we've already read.
            let mut limit = io::Cursor::new(&buf);

            let settings = match webtransport_proto::Settings::decode(&mut limit) {
                Ok(settings) => settings,
                Err(webtransport_proto::SettingsError::UnexpectedEnd) => continue, // More data needed.
                Err(e) => return Err(e.into()),
            };

            if settings.supports_webtransport() == 0 {
                return Err(SettingsError::WebTransportUnsupported);
            }

            return Ok(recv);
        }
    }

    async fn open(conn: &quinn::Connection) -> Result<quinn::SendStream, SettingsError> {
        let mut settings = webtransport_proto::Settings::default();
        settings.enable_webtransport(1);

        let mut buf = Vec::new();
        settings.encode(&mut buf);

        let mut send = conn.open_uni().await?;
        send.write_all(&buf).await?;

        Ok(send)
    }
}
