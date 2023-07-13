use std::io;

use thiserror::Error;

use crate::{h3, settings, Session};

// Utility method to resolve a given URL.
pub async fn dial(client: &quinn::Endpoint, uri: &http::Uri) -> Result<quinn::Connecting, quinn::ConnectError> {
	let authority = uri
		.authority()
		.ok_or(quinn::ConnectError::InvalidDnsName("".to_string()))?;

	// TODO error on username:password in host
	let host = authority.host();
	let port = authority.port().map(|p| p.as_u16()).unwrap_or(443);

	//let temp = host.clone(); // not sure why tokio takes ownership
	let mut remotes = match tokio::net::lookup_host((host, port)).await {
		Ok(remotes) => remotes,
		Err(_) => return Err(quinn::ConnectError::InvalidDnsName(host.to_string())),
	};

	let remote = match remotes.next() {
		Some(remote) => remote,
		None => return Err(quinn::ConnectError::InvalidDnsName(host.to_string())),
	};

	client.connect(remote, host)
}

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
	SettingsError(#[from] settings::SettingsError),

	#[error("failed to exchange h3 connect")]
	ConnectError(#[from] h3::ConnectError),
}

pub async fn connect(conn: quinn::Connection, uri: &http::Uri) -> Result<Session, ConnectError> {
	let control = settings::connect(&conn).await?;
	let mut connect = conn.open_bi().await?;

	let mut buf = Vec::new();
	h3::ConnectRequest { uri: uri.clone() }.encode(&mut buf); // TODO avoid clone
	connect.0.write_all(&buf).await?;

	buf.clear();

	loop {
		// Read more data into the buffer.
		let chunk = connect.1.read_chunk(usize::MAX, true).await?;
		let chunk = chunk.ok_or(ConnectError::UnexpectedEnd)?;
		buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

		let mut limit = io::Cursor::new(&buf);

		let res = match h3::ConnectResponse::decode(&mut limit) {
			Ok(res) => res,
			Err(h3::ConnectError::UnexpectedEnd(_)) => continue,
			Err(e) => return Err(e.into()),
		};

		if res.status != http::StatusCode::OK {
			return Err(h3::ConnectError::ErrorStatus(res.status).into());
		}

		let session = Session::new(conn, control, connect);

		return Ok(session);
	}
}
