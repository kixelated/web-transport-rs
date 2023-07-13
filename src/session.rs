use quinn_proto::{coding::Codec, VarInt};

use quinn::{RecvStream, SendStream};
type BidiStream = (SendStream, RecvStream);

use std::{
	ops::{Deref, DerefMut},
	sync::Arc,
};

use super::h3;

#[derive(Clone)]
pub struct Session {
	conn: quinn::Connection,
	session_id: VarInt,

	// Keep a reference to the control and connect stream to avoid closing them.
	#[allow(dead_code)]
	control: Arc<BidiStream>,
	#[allow(dead_code)]
	connect: Arc<BidiStream>,

	// Cache the headers in front of each stream we open.
	header_uni: Vec<u8>,
	header_bi: Vec<u8>,
}

impl Session {
	pub(crate) fn new(conn: quinn::Connection, control: BidiStream, connect: BidiStream) -> Self {
		// Cache some encoded values for better performance.
		let session_id = VarInt::from(connect.0.id());

		// Cache the tiny header we write in front of each stream we open.
		let mut header_uni = Vec::new();
		h3::StreamUni::WEBTRANSPORT.encode(&mut header_uni);
		session_id.encode(&mut header_uni);

		let mut header_bi = Vec::new();
		h3::Frame::WEBTRANSPORT.encode(&mut header_bi);
		session_id.encode(&mut header_bi);

		Self {
			conn,
			control: Arc::new(control),
			connect: Arc::new(connect),

			session_id,
			header_uni,
			header_bi,
		}
	}

	pub async fn open_uni(&self) -> Result<quinn::SendStream, quinn::WriteError> {
		let mut send = self.conn.open_uni().await?;
		send.write_all(&self.header_uni).await?;
		Ok(send)
	}

	pub async fn open_bi(&self) -> Result<(quinn::SendStream, quinn::RecvStream), quinn::WriteError> {
		let (mut send, recv) = self.conn.open_bi().await?;
		send.write_all(&self.header_bi).await?;
		Ok((send, recv))
	}

	pub async fn accept_uni(&self) -> Result<quinn::RecvStream, quinn::ReadExactError> {
		loop {
			let mut recv = self.conn.accept_uni().await.map_err(quinn::ReadError::ConnectionLost)?;

			let typ = h3::StreamUni(read_varint(&mut recv).await?);
			if typ.is_reserved() {
				// HTTP/3 reserved streams are ignored.
				continue;
			}

			if typ != h3::StreamUni::WEBTRANSPORT {
				return Err(quinn::ReadError::UnknownStream.into());
			}

			let session_id = read_varint(&mut recv).await?;
			if session_id != self.session_id {
				// TODO return a better error message: unknown session
				return Err(quinn::ReadError::UnknownStream.into());
			}

			return Ok(recv);
		}
	}

	pub async fn accept_bi(&self) -> Result<(quinn::SendStream, quinn::RecvStream), quinn::ReadExactError> {
		let (send, mut recv) = self.conn.accept_bi().await.map_err(quinn::ReadError::ConnectionLost)?;

		let typ = h3::Frame(read_varint(&mut recv).await?);
		if typ != h3::Frame::WEBTRANSPORT {
			return Err(quinn::ReadError::UnknownStream.into());
		}

		let session_id = read_varint(&mut recv).await?;
		if session_id != self.session_id {
			// TODO return a better error message: unknown session
			return Err(quinn::ReadError::UnknownStream.into());
		}

		Ok((send, recv))
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
async fn read_varint(stream: &mut quinn::RecvStream) -> Result<VarInt, quinn::ReadExactError> {
	// 8 bytes is the max size of a varint
	let mut buf = [0; 8];

	// Read the first byte because it includes the length.
	stream.read_exact(&mut buf[0..1]).await?;

	// 0b00 = 1, 0b01 = 2, 0b10 = 4, 0b11 = 8
	let size = 1 << (buf[0] >> 6);
	stream.read_exact(&mut buf[1..size]).await?;

	// Use a cursor to read the varint on the stack.
	let mut cursor = std::io::Cursor::new(&buf[..size]);
	let v = VarInt::decode(&mut cursor).unwrap();

	Ok(v)
}
