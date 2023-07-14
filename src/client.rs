use async_std::net::ToSocketAddrs;
use std::io;
use thiserror::Error;

use crate::{h3, Session};

/// An error returned when connecting to a WebTransport endpoint.
#[derive(Error, Debug)]
pub enum ConnectError {
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

    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),

    #[error("quic error: {0}")]
    QuicError(#[from] quinn::ConnectError),
}

/// Connect to a WebTransport server at the given URI.
/// The URI must be of the form `https://host:port/path` or else the server will reject it.
/// Returns a [`Session`] which is a wrapper over [`quinn::Connection`].
pub async fn connect(client: &quinn::Endpoint, uri: &http::Uri) -> Result<Session, ConnectError> {
    let authority = uri
        .authority()
        .ok_or(ConnectError::InvalidDnsName("".to_string()))?;

    // TODO error on username:password in host
    let host = authority.host();
    let port = authority.port().map(|p| p.as_u16()).unwrap_or(443);

    // Look up the DNS entry.
    let mut remotes = match (host, port).to_socket_addrs().await {
        Ok(remotes) => remotes,
        Err(_) => return Err(ConnectError::InvalidDnsName(host.to_string())),
    };

    // Return the first entry.
    let remote = match remotes.next() {
        Some(remote) => remote,
        None => return Err(ConnectError::InvalidDnsName(host.to_string())),
    };

    // Connect to the server using the addr we just resolved.
    let conn = client.connect(remote, host)?;
    let conn = conn.await?;

    // Connect with the connection we established.
    connect_with(conn, uri).await
}

/// Connect using an established QUIC connection if you want to create the connection yourself.
/// This will only work with a brand new QUIC connection using the HTTP/3 ALPN.
pub async fn connect_with(
    conn: quinn::Connection,
    uri: &http::Uri,
) -> Result<Session, ConnectError> {
    // Perform the H3 handshake by sending/reciving SETTINGS frames.
    let control = h3::settings(&conn).await?;

    // Create a new stream that will be used to send the CONNECT frame.
    let mut connect = conn.open_bi().await?;

    // Create a new CONNECT request that we'll send using HTTP/3
    // TODO avoid cloning here
    let _req = h3::ConnectRequest { uri: uri.clone() };

    // Encode our connect request into a buffer and write it to the stream.
    let mut buf = Vec::new();
    h3::ConnectRequest { uri: uri.clone() }.encode(&mut buf); // TODO avoid clone
    connect.0.write_all(&buf).await?;

    buf.clear();

    // Read the response from the server, buffering more data until we get a full response.
    loop {
        // Read more data into the buffer.
        // We use the chunk API here instead of read_buf literally just to return a quinn::ReadError instead of io::Error.
        let chunk = connect.1.read_chunk(usize::MAX, true).await?;
        let chunk = chunk.ok_or(ConnectError::UnexpectedEnd)?;
        buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

        // Create a cursor that will tell us how much of the buffer was read.
        let mut limit = io::Cursor::new(&buf);

        // Try to decode the response.
        let res = match h3::ConnectResponse::decode(&mut limit) {
            // It worked, return it.
            Ok(res) => res,

            // We didn't have enough data in the buffer, so we'll read more and try again.
            Err(h3::ConnectError::UnexpectedEnd(_)) => continue,

            // Some other fatal error.
            Err(e) => return Err(e.into()),
        };

        // Throw an error if we didn't get a 200 OK.
        if res.status != http::StatusCode::OK {
            return Err(h3::ConnectError::ErrorStatus(res.status).into());
        }

        // Return the resulting session with a reference to the control/connect streams.
        // If either stream is closed, then the session will be closed, so we need to keep them around.
        let session = Session::new(conn, control, connect);

        return Ok(session);
    }
}
