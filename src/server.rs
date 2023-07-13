use std::io;

use crate::{h3, settings, Session};

use quinn::{RecvStream, SendStream};
type BidiStream = (SendStream, RecvStream);

use thiserror::Error;

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
	SettingsError(#[from] settings::SettingsError),

	#[error("failed to exchange h3 connect")]
	ConnectError(#[from] h3::ConnectError),
}

// Complete the H3 handshake and return the WebTransport CONNECT
// NOTE: This return a Request object so you have to explicitly ok/error to get the underlying session.
pub async fn accept(conn: quinn::Connection) -> Result<Request, AcceptError> {
	let control = settings::connect(&conn).await?;

	let mut connect = conn.accept_bi().await?;
	let mut buf = Vec::new();

	loop {
		// Read more data into the buffer.
		let chunk = connect.1.read_chunk(usize::MAX, true).await?;
		let chunk = chunk.ok_or(AcceptError::UnexpectedEnd)?;
		buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

		let mut limit = io::Cursor::new(&buf);

		let req = match h3::ConnectRequest::decode(&mut limit) {
			Ok(req) => req,
			Err(h3::ConnectError::UnexpectedEnd(_)) => continue,
			Err(e) => return Err(e.into()),
		};

		let req = Request {
			conn,
			control,
			connect,
			uri: req.uri,
		};

		return Ok(req);
	}
}

pub struct Request {
	conn: quinn::Connection,
	control: BidiStream,
	connect: BidiStream,
	uri: http::Uri,
}

impl Request {
	pub fn uri(&self) -> &http::Uri {
		&self.uri
	}

	pub async fn ok(mut self) -> Result<Session, quinn::WriteError> {
		self.respond(http::StatusCode::OK).await?;
		let conn = Session::new(self.conn, self.control, self.connect);
		Ok(conn)
	}

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
