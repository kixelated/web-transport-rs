use crate::{Connect, ConnectError, Session, Settings, SettingsError};

use thiserror::Error;

/// An error returned when receiving a new WebTransport session.
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("connection error")]
    Connection(#[from] quinn::ConnectionError),

    #[error("failed to write")]
    WriteError(#[from] quinn::WriteError),

    #[error("failed to read")]
    ReadError(#[from] quinn::ReadError),

    #[error("failed to exchange h3 settings")]
    SettingsError(#[from] SettingsError),

    #[error("failed to exchange h3 connect")]
    ConnectError(#[from] ConnectError),
}

/// Accept a new WebTransport session from a client.
/// Returns a [`Request`] which is then used to accept or reject the session based on the URI.
pub async fn accept(conn: quinn::Connection) -> Result<Request, ServerError> {
    // Perform the H3 handshake by sending/reciving SETTINGS frames.
    let settings = Settings::connect(&conn).await?;

    // Accept the CONNECT request but don't send a response yet.
    let connect = Connect::accept(&conn).await?;

    // Return the resulting request with a reference to the settings/connect streams.
    Ok(Request {
        conn,
        settings,
        connect,
    })
}

/// A mostly complete WebTransport handshake, just awaiting the server's decision on whether to accept or reject the session based on the URI.
pub struct Request {
    conn: quinn::Connection,
    settings: Settings,
    connect: Connect,
}

impl Request {
    /// Returns the URI provided by the client.
    pub fn uri(&self) -> &http::Uri {
        self.connect.uri()
    }

    /// Accept the session, returning a 200 OK.
    pub async fn ok(mut self) -> Result<Session, quinn::WriteError> {
        self.connect.respond(http::StatusCode::OK).await?;
        Ok(Session::new(self.conn, self.settings, self.connect))
    }

    /// Reject the session, returing your favorite HTTP status code.
    pub async fn close(mut self, status: http::StatusCode) -> Result<(), quinn::WriteError> {
        self.connect.respond(status).await?;
        Ok(())
    }
}
