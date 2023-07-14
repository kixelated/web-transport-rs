use std::io;

use crate::{h3, Session};

use quinn::{RecvStream, SendStream};
type BidiStream = (SendStream, RecvStream);

use thiserror::Error;

/// An error returned when receiving a new WebTransport session.
#[derive(Error, Debug)]
pub enum AcceptError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("connection error")]
    Connection(#[from] quinn::ConnectionError),

    #[error("failed to write")]
    WriteError(#[from] quinn::WriteError),

    #[error("failed to read")]
    ReadError(#[from] quinn::ReadError),

    #[error("failed to exchange h3 settings")]
    SettingsError(#[from] h3::SettingsError),

    #[error("failed to exchange h3 connect")]
    ConnectError(#[from] h3::ConnectError),
}

/// Accept a new WebTransport session from a client.
/// Returns a [`Request`] which is then used to accept or reject the session based on the URI.
pub async fn accept(conn: quinn::Connection) -> Result<Request, AcceptError> {
    // Perform the H3 handshake by sending/reciving SETTINGS frames.
    let control = h3::settings(&conn).await?;

    // Accept the stream that will be used to send the HTTP CONNECT request.
    // If they try to send any other type of HTTP request, we will error out.
    let mut connect = conn.accept_bi().await?;
    let mut buf = Vec::new();

    // Read the request from the client, buffering more data until we get a full response.
    loop {
        // Read more data into the buffer.
        // We use the chunk API here instead of read_buf literally just to return a quinn::ReadError instead of io::Error.
        let chunk = connect.1.read_chunk(usize::MAX, true).await?;
        let chunk = chunk.ok_or(AcceptError::UnexpectedEnd)?;
        buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

        // Create a cursor that will tell us how much of the buffer was read.
        let mut limit = io::Cursor::new(&buf);

        // Try to decode the request.
        let req = match h3::ConnectRequest::decode(&mut limit) {
            // It worked, return it.
            Ok(req) => req,

            // We didn't have enough data in the buffer, so we'll read more and try again.
            Err(h3::ConnectError::UnexpectedEnd(_)) => continue,

            // Some other fatal error.
            Err(e) => return Err(e.into()),
        };

        // Return the resulting request with a reference to the control/connect streams.
        // If either stream is closed, then the session will be closed, so we need to keep them around.
        let req = Request {
            conn,
            control,
            connect,
            uri: req.uri,
        };

        return Ok(req);
    }
}

/// A mostly complete WebTransport handshake, just awaiting the server's decision on whether to accept or reject the session based on the URI.
pub struct Request {
    conn: quinn::Connection,
    control: BidiStream,
    connect: BidiStream,
    uri: http::Uri,
}

impl Request {
    /// Returns the URI provided by the client.
    pub fn uri(&self) -> &http::Uri {
        &self.uri
    }

    /// Accept the session, returning a 200 OK.
    pub async fn ok(mut self) -> Result<Session, quinn::WriteError> {
        self.respond(http::StatusCode::OK).await?;
        let conn = Session::new(self.conn, self.control, self.connect);
        Ok(conn)
    }

    /// Reject the session, returing your favorite HTTP status code.
    pub async fn close(mut self, status: http::StatusCode) -> Result<(), quinn::WriteError> {
        self.respond(status).await?;
        Ok(())
    }

    async fn respond(&mut self, status: http::StatusCode) -> Result<(), quinn::WriteError> {
        let resp = h3::ConnectResponse { status };

        let mut buf = Vec::new();
        resp.encode(&mut buf);

        self.connect.0.write_all(&buf).await?;

        Ok(())
    }
}
