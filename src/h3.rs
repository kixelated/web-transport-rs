use std::{
	collections::HashMap,
	ops::{Deref, DerefMut},
	str::FromStr,
};

use bytes::{Buf, BufMut};

use quinn_proto::coding::{self, Codec};
pub use quinn_proto::VarInt;

use super::qpack;

use thiserror::Error;

// Sent as the first byte of a unidirectional stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamUni(pub VarInt);

macro_rules! streams_uni {
    {$($name:ident = $val:expr,)*} => {
        impl StreamUni {
            $(pub const $name: StreamUni = StreamUni(VarInt::from_u32($val));)*
        }
    }
}

streams_uni! {
	CONTROL = 0x00,
	WEBTRANSPORT = 0x54,
}

impl StreamUni {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, coding::UnexpectedEnd> {
		Ok(StreamUni(VarInt::decode(buf)?))
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		self.0.encode(buf)
	}

	pub fn is_reserved(&self) -> bool {
		let val = self.0.into_inner();
		if val < 0x21 {
			return false;
		}

		(val - 0x21) % 0x1f == 0
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Frame(pub VarInt);

macro_rules! frames {
    {$($name:ident = $val:expr,)*} => {
        impl Frame {
            $(pub const $name: Frame = Frame(VarInt::from_u32($val));)*
        }
    }
}

// Sent at the start of bidirectional streams.
frames! {
	DATA = 0x00,
	HEADERS = 0x01,
	QPACK_ENCODER = 0x02,
	QPACK_DECODER = 0x03,
	SETTINGS = 0x04,
	WEBTRANSPORT = 0x41,
}

impl Frame {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, coding::UnexpectedEnd> {
		Ok(Frame(VarInt::decode(buf)?))
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		self.0.encode(buf)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Setting(pub VarInt);

impl Setting {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, coding::UnexpectedEnd> {
		Ok(Setting(VarInt::decode(buf)?))
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		self.0.encode(buf)
	}
}

macro_rules! settings {
    {$($name:ident = $val:expr,)*} => {
        impl Setting {
            $(pub const $name: Setting = Setting(VarInt::from_u32($val));)*
        }
    }
}

settings! {
	// Both of these are required for WebTransport
	ENABLE_CONNECT_PROTOCOL = 0x8,
	ENABLE_DATAGRAM = 0x33,
	ENABLE_DATAGRAM_DEPRECATED = 0xFFD277, // still used by Chrome

	// Removed in draft 06
	WEBTRANSPORT_ENABLE_DEPRECATED = 0x2b603742,
	WEBTRANSPORT_MAX_SESSIONS_DEPRECATED = 0x2b603743,

	// New way to enable WebTransport
	WEBTRANSPORT_MAX_SESSIONS = 0xc671706a,
}

#[derive(Error, Debug)]
pub enum SettingsError {
	#[error("unexpected end of input")]
	UnexpectedEnd(#[from] coding::UnexpectedEnd),

	#[error("unexpected stream type {0:?}")]
	UnexpectedStreamType(StreamUni),

	#[error("unexpected frame {0:?}")]
	UnexpectedFrame(Frame),

	#[error("invalid size")]
	InvalidSize,
}

#[derive(Default, Debug)]
pub struct Settings(HashMap<Setting, VarInt>);

impl Settings {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, SettingsError> {
		let typ = StreamUni::decode(buf)?;
		if typ != StreamUni::CONTROL {
			return Err(SettingsError::UnexpectedStreamType(typ));
		}

		let typ = Frame::decode(buf)?;
		if typ != Frame::SETTINGS {
			return Err(SettingsError::UnexpectedFrame(typ));
		}

		let size = VarInt::decode(buf)?;

		let mut limit = bytes::Buf::take(buf, size.into_inner() as usize);
		if limit.remaining() < limit.limit() {
			return Err(SettingsError::UnexpectedEnd(coding::UnexpectedEnd));
		}

		let mut settings = Settings::default();
		while limit.has_remaining() {
			let id = Setting::decode(&mut limit).map_err(|coding::UnexpectedEnd| SettingsError::InvalidSize)?;
			let value = VarInt::decode(&mut limit).map_err(|coding::UnexpectedEnd| SettingsError::InvalidSize)?;
			settings.0.insert(id, value);
		}

		Ok(settings)
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		StreamUni::CONTROL.encode(buf);
		Frame::SETTINGS.encode(buf);

		// Encode to a temporary buffer so we can learn the length.
		let mut tmp = Vec::new();
		for (id, value) in &self.0 {
			id.encode(&mut tmp);
			value.encode(&mut tmp);
		}

		VarInt::from_u32(tmp.len() as u32).encode(buf);
		buf.put_slice(&tmp);
	}

	pub fn enable_webtransport(&mut self, max_sessions: u32) {
		let max = VarInt::from_u32(max_sessions);

		self.insert(Setting::ENABLE_CONNECT_PROTOCOL, VarInt::from_u32(1));
		self.insert(Setting::ENABLE_DATAGRAM, VarInt::from_u32(1));
		self.insert(Setting::ENABLE_DATAGRAM_DEPRECATED, VarInt::from_u32(1));
		self.insert(Setting::WEBTRANSPORT_MAX_SESSIONS, max);

		// TODO remove when 07 is in the wild
		self.insert(Setting::WEBTRANSPORT_MAX_SESSIONS_DEPRECATED, max);
		self.insert(Setting::WEBTRANSPORT_ENABLE_DEPRECATED, VarInt::from_u32(1));
	}

	// Returns the maximum number of sessions supported.
	pub fn supports_webtransport(&self) -> u64 {
		match self.get(&Setting::ENABLE_CONNECT_PROTOCOL) {
			Some(v) if v.into_inner() == 1 => {}
			_ => return 0,
		};

		match self
			.get(&Setting::ENABLE_DATAGRAM)
			.or(self.get(&Setting::ENABLE_DATAGRAM_DEPRECATED))
		{
			Some(v) if v.into_inner() == 1 => {}
			_ => return 0,
		};

		// The deprecated (before draft-07) way of enabling WebTransport was to send two parameters.
		// Both would send ENABLE=1 and the server would send MAX_SESSIONS=N to limit the sessions.
		// Now both just send MAX_SESSIONS, and a non-zero value means WebTransport is enabled.

		match self.get(&Setting::WEBTRANSPORT_MAX_SESSIONS) {
			Some(max) => max.into_inner(),

			// Only the server is allowed to set this deprecated... but we don't care.
			None => match self.get(&Setting::WEBTRANSPORT_MAX_SESSIONS_DEPRECATED) {
				Some(max) => max.into_inner(),

				// If this is set but not MAX_SESSIONS, it means a client sent it and we're too lazy to check.
				None => match self.get(&Setting::WEBTRANSPORT_ENABLE_DEPRECATED) {
					Some(v) if v.into_inner() == 1 => 1,
					_ => 0,
				},
			},
		}
	}
}

impl Deref for Settings {
	type Target = HashMap<Setting, VarInt>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DerefMut for Settings {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

#[derive(Error, Debug)]
pub enum ConnectError {
	#[error("unexpected end of input")]
	UnexpectedEnd(#[from] coding::UnexpectedEnd),

	#[error("qpack error")]
	QpackError(#[from] qpack::DecodeError),

	#[error("unexpected frame {0:?}")]
	UnexpectedFrame(Frame),

	#[error("invalid method")]
	InvalidMethod(#[from] http::method::InvalidMethod),

	#[error("invalid uri")]
	InvalidUri(#[from] http::uri::InvalidUri),

	#[error("invalid uri parts")]
	InvalidUriParts(#[from] http::uri::InvalidUriParts),

	#[error("invalid status")]
	InvalidStatus(#[from] http::status::InvalidStatusCode),

	#[error("expected 200, got: {0:?}")]
	WrongStatus(Option<http::StatusCode>),

	#[error("expected connect, got: {0:?}")]
	WrongMethod(Option<http::method::Method>),

	#[error("expected https, got: {0:?}")]
	WrongScheme(Option<http::uri::Scheme>),

	#[error("expected authority header")]
	WrongAuthority,

	#[error("expected webtransport, got: {0:?}")]
	WrongProtocol(Option<String>),

	#[error("non-200 status: {0:?}")]
	ErrorStatus(http::StatusCode),
}

#[derive(Debug)]
pub struct ConnectRequest {
	pub uri: http::Uri,
}

impl ConnectRequest {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, ConnectError> {
		let typ = Frame::decode(buf)?;
		if typ != Frame::HEADERS {
			return Err(ConnectError::UnexpectedFrame(typ));
		}

		let size = VarInt::decode(buf)?;
		let mut limit = Buf::take(buf, size.into_inner() as usize);
		if limit.limit() > limit.remaining() {
			// Not enough data in the buffer
			return Err(ConnectError::UnexpectedEnd(coding::UnexpectedEnd));
		}

		let headers = qpack::Headers::decode(&mut limit)?;

		let mut parts = http::uri::Parts::default();
		parts.scheme = headers.get(":scheme").map(|scheme| scheme.try_into()).transpose()?;
		parts.authority = headers.get(":authority").map(|auth| auth.try_into()).transpose()?;
		parts.path_and_query = headers.get(":path").map(|path| path.try_into()).transpose()?;
		let uri = http::Uri::from_parts(parts)?;

		// Validate the headers
		match headers.get(":method").map(|method| method.try_into()).transpose()? {
			Some(http::Method::CONNECT) => (),
			o => return Err(ConnectError::WrongMethod(o)),
		};

		let protocol = headers.get(":protocol");
		if protocol != Some("webtransport") {
			return Err(ConnectError::WrongProtocol(protocol.map(|s| s.to_string())));
		}

		if uri.scheme() != Some(&http::uri::Scheme::HTTPS) {
			return Err(ConnectError::WrongScheme(uri.scheme().cloned()));
		}

		if uri.authority().is_none() {
			return Err(ConnectError::WrongAuthority);
		}

		Ok(Self { uri })
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		let mut headers = qpack::Headers::default();
		headers.set(":method", "CONNECT");

		if let Some(scheme) = self.uri.scheme() {
			headers.set(":scheme", scheme.as_str());
		}

		if let Some(host) = self.uri.authority() {
			headers.set(":authority", host.as_str());
		}

		headers.set(":path", self.uri.path());
		headers.set(":protocol", "webtransport");

		// Use a temporary buffer so we can compute the size.
		let mut tmp = Vec::new();
		headers.encode(&mut tmp);
		let size = VarInt::from_u32(tmp.len() as u32);

		Frame::HEADERS.encode(buf);
		size.encode(buf);
		buf.put_slice(&tmp);
	}
}

#[derive(Debug)]
pub struct ConnectResponse {
	pub status: http::status::StatusCode,
}

impl ConnectResponse {
	pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, ConnectError> {
		let typ = Frame::decode(buf)?;
		if typ != Frame::HEADERS {
			return Err(ConnectError::UnexpectedFrame(typ));
		}

		let size = VarInt::decode(buf)?;

		let mut limit = Buf::take(buf, size.into_inner() as usize);
		if limit.limit() > limit.remaining() {
			return Err(ConnectError::UnexpectedEnd(coding::UnexpectedEnd));
		}

		let headers = qpack::Headers::decode(&mut limit)?;

		let status = match headers.get(":status").map(http::StatusCode::from_str).transpose()? {
			Some(status) if status.is_success() => status,
			o => return Err(ConnectError::WrongStatus(o)),
		};

		Ok(Self { status })
	}

	pub fn encode<B: BufMut>(&self, buf: &mut B) {
		let mut headers = qpack::Headers::default();
		headers.set(":status", self.status.as_str());
		headers.set(":protocol", "webtransport");
		headers.set(":sec-webtransport-http3-draft", "draft02");

		// Use a temporary buffer so we can compute the size.
		let mut tmp = Vec::new();
		headers.encode(&mut tmp);
		let size = VarInt::from_u32(tmp.len() as u32);

		Frame::HEADERS.encode(buf);
		size.encode(buf);
		buf.put_slice(&tmp);
	}
}
