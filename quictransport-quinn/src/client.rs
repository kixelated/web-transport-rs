use tokio::net::lookup_host;
use url::Url;

use crate::Session;

/// An error returned when connecting to a WebTransport endpoint.
#[derive(thiserror::Error, Debug, Clone)]
pub enum ClientError {
    #[error("connection error: {0}")]
    Connection(#[from] quinn::ConnectionError),

    #[error("connect error: {0}")]
    Connect(#[from] quinn::ConnectError),

    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),
}

/// Connect to a server at the given URL.
pub async fn connect(client: &quinn::Endpoint, url: &Url) -> Result<Session, ClientError> {
    // TODO error on username:password in host
    let host = url
        .host()
        .ok_or_else(|| ClientError::InvalidDnsName("".to_string()))?
        .to_string();

    let port = url.port().unwrap_or(443);

    // Look up the DNS entry.
    let mut remotes = match lookup_host((host.clone(), port)).await {
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

    Ok(conn.into())
}
