use std::io;

use thiserror::Error;
use tokio::try_join;

use super::h3;

use quinn::{RecvStream, SendStream};
type BidiStream = (SendStream, RecvStream);

#[derive(Error, Debug)]
pub enum SettingsError {
	#[error("unexpected end of stream")]
	UnexpectedEnd,

	#[error("connection error")]
	Connection(#[from] quinn::ConnectionError),

	#[error("failed to write")]
	WriteError(#[from] quinn::WriteError),

	#[error("failed to read")]
	ReadError(#[from] quinn::ReadError),

	#[error("failed to read settings")]
	SettingsError(#[from] h3::SettingsError),

	#[error("webtransport unsupported")]
	WebTransportUnsupported,
}

// Establish the H3 connection.
pub async fn connect(conn: &quinn::Connection) -> Result<BidiStream, SettingsError> {
	let recv = read_settings(conn);
	let send = write_settings(conn);

	// Run both tasks concurrently until one errors or they both complete.
	let control = try_join!(send, recv)?;
	Ok(control)
}

async fn read_settings(conn: &quinn::Connection) -> Result<quinn::RecvStream, SettingsError> {
	let mut recv = conn.accept_uni().await?;
	let mut buf = Vec::new();

	loop {
		// Read more data into the buffer.
		let chunk = recv.read_chunk(usize::MAX, true).await?;
		let chunk = chunk.ok_or(SettingsError::UnexpectedEnd)?;
		buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

		// Look at the buffer we've already read.
		let mut limit = io::Cursor::new(&buf);

		let settings = match h3::Settings::decode(&mut limit) {
			Ok(settings) => settings,
			Err(h3::SettingsError::UnexpectedEnd(_)) => continue, // More data needed.
			Err(e) => return Err(e.into()),
		};

		if settings.supports_webtransport() == 0 {
			return Err(SettingsError::WebTransportUnsupported);
		}

		return Ok(recv);
	}
}

async fn write_settings(conn: &quinn::Connection) -> Result<quinn::SendStream, SettingsError> {
	let mut settings = h3::Settings::default();
	settings.enable_webtransport(1);

	let mut buf = Vec::new();
	settings.encode(&mut buf);

	let mut send = conn.open_uni().await?;
	send.write_all(&buf).await?;

	Ok(send)
}
