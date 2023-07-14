use std::{
    collections::HashMap,
    io,
    ops::{Deref, DerefMut},
};

use bytes::{Buf, BufMut};

use futures::try_join;
use thiserror::Error;

use quinn::{RecvStream, SendStream};
type BidiStream = (SendStream, RecvStream);

use quinn_proto::coding::{self, Codec};
use quinn_proto::VarInt;

use super::{Frame, StreamUni};

#[derive(Error, Debug)]
pub enum SettingsError {
    #[error("unexpected end of input")]
    UnexpectedEnd(#[from] coding::UnexpectedEnd),

    #[error("connection error")]
    Connection(#[from] quinn::ConnectionError),

    #[error("failed to write")]
    WriteError(#[from] quinn::WriteError),

    #[error("failed to read")]
    ReadError(#[from] quinn::ReadError),

    #[error("unexpected stream type {0:?}")]
    UnexpectedStreamType(StreamUni),

    #[error("unexpected frame {0:?}")]
    UnexpectedFrame(Frame),

    #[error("invalid size")]
    InvalidSize,

    #[error("webtransport unsupported")]
    WebTransportUnsupported,
}

// Establish the H3 connection.
pub async fn settings(conn: &quinn::Connection) -> Result<BidiStream, SettingsError> {
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
        let chunk = chunk.ok_or(SettingsError::UnexpectedEnd(coding::UnexpectedEnd))?;
        buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

        // Look at the buffer we've already read.
        let mut limit = io::Cursor::new(&buf);

        let settings = match Settings::decode(&mut limit) {
            Ok(settings) => settings,
            Err(SettingsError::UnexpectedEnd(_)) => continue, // More data needed.
            Err(e) => return Err(e),
        };

        if settings.supports_webtransport() == 0 {
            return Err(SettingsError::WebTransportUnsupported);
        }

        return Ok(recv);
    }
}

async fn write_settings(conn: &quinn::Connection) -> Result<quinn::SendStream, SettingsError> {
    let mut settings = Settings::default();
    settings.enable_webtransport(1);

    let mut buf = Vec::new();
    settings.encode(&mut buf);

    let mut send = conn.open_uni().await?;
    send.write_all(&buf).await?;

    Ok(send)
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

// A map of settings to values.
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
            let id = Setting::decode(&mut limit)
                .map_err(|coding::UnexpectedEnd| SettingsError::InvalidSize)?;
            let value = VarInt::decode(&mut limit)
                .map_err(|coding::UnexpectedEnd| SettingsError::InvalidSize)?;
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
