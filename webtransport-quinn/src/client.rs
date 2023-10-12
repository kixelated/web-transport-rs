use async_std::net::ToSocketAddrs;
use thiserror::Error;
use url::Url;

use crate::{Connect, ConnectError, Session, Settings, SettingsError};

/// An error returned when connecting to a WebTransport endpoint.
#[derive(Error, Debug)]
pub enum ClientError {
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

    #[error("quic error: {0}")]
    QuicError(#[from] quinn::ConnectError),

    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),
}

/// Connect to a WebTransport server at the given URL.
/// The UR: must be of the form `https://host:port/path` or else the server will reject it.
/// Returns a [`Session`] which is a wrapper over [`quinn::Connection`].
pub async fn connect(client: &quinn::Endpoint, url: &Url) -> Result<Session, ClientError> {
    // TODO error on username:password in host
    let host = url
        .host()
        .ok_or_else(|| ClientError::InvalidDnsName("".to_string()))?
        .to_string();

    let port = url.port().unwrap_or(443);

    // Look up the DNS entry.
    let mut remotes = match (host.as_str(), port).to_socket_addrs().await {
        Ok(remotes) => remotes,
        Err(_) => return Err(ClientError::InvalidDnsName(host)),
    };

    // Return the first entry.
    let remote = match remotes.next() {
        Some(remote) => remote,
        None => return Err(ClientError::InvalidDnsName(host)),
    };

    // Connect to the server using the addr we just resolved.
    let conn = client.connect(remote, &host)?;
    let conn = conn.await?;

    // Connect with the connection we established.
    connect_with(conn, url).await
}

/// Connect using an established QUIC connection if you want to create the connection yourself.
/// This will only work with a brand new QUIC connection using the HTTP/3 ALPN.
pub async fn connect_with(conn: quinn::Connection, url: &Url) -> Result<Session, ClientError> {
    // Perform the H3 handshake by sending/reciving SETTINGS frames.
    let settings = Settings::connect(&conn).await?;

    // Send the HTTP/3 CONNECT request.
    let connect = Connect::open(&conn, url).await?;

    // Return the resulting session with a reference to the control/connect streams.
    // If either stream is closed, then the session will be closed, so we need to keep them around.
    let session = Session::new(conn, settings, connect);

    Ok(session)
}
