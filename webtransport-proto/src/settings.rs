use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use bytes::{Buf, BufMut};

use thiserror::Error;

use super::{Frame, StreamUni, VarInt, VarIntUnexpectedEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Setting(pub VarInt);

impl Setting {
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, VarIntUnexpectedEnd> {
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
    // These are for HTTP/3 and we can ignore them
    QPACK_MAX_TABLE_CAPACITY = 0x1, // default is 0, which disables QPACK dynamic table
    MAX_FIELD_SECTION_SIZE = 0x6,
    QPACK_BLOCKED_STREAMS = 0x7,

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
    UnexpectedEnd,

    #[error("unexpected stream type {0:?}")]
    UnexpectedStreamType(StreamUni),

    #[error("unexpected frame {0:?}")]
    UnexpectedFrame(Frame),

    #[error("invalid size")]
    InvalidSize,
}

// A map of settings to values.
#[derive(Default, Debug)]
pub struct Settings(HashMap<Setting, VarInt>);

impl Settings {
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, SettingsError> {
        let typ = StreamUni::decode(buf).map_err(|_| SettingsError::UnexpectedEnd)?;
        if typ != StreamUni::CONTROL {
            return Err(SettingsError::UnexpectedStreamType(typ));
        }

        let (typ, mut data) = Frame::read(buf).map_err(|_| SettingsError::UnexpectedEnd)?;
        if typ != Frame::SETTINGS {
            return Err(SettingsError::UnexpectedFrame(typ));
        }

        let mut settings = Settings::default();
        while data.has_remaining() {
            // These return a different error because retrying won't help.
            let id = Setting::decode(&mut data).map_err(|_| SettingsError::InvalidSize)?;
            let value = VarInt::decode(&mut data).map_err(|_| SettingsError::InvalidSize)?;
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
        // Sent by Chrome 114.0.5735.198 (July 19, 2023)
        // Setting(1): 65536,              // qpack_max_table_capacity
        // Setting(6): 16384,              // max_field_section_size
        // Setting(7): 100,                // qpack_blocked_streams
        // Setting(51): 1,                 // enable_datagram
        // Setting(16765559): 1            // enable_datagram_deprecated
        // Setting(727725890): 1,          // webtransport_max_sessions_deprecated
        // Setting(4445614305): 454654587, // grease

        // NOTE: The presence of ENABLE_WEBTRANSPORT implies ENABLE_CONNECT is supported.

        let datagram = self
            .get(&Setting::ENABLE_DATAGRAM)
            .or(self.get(&Setting::ENABLE_DATAGRAM_DEPRECATED))
            .map(|v| v.into_inner());

        if datagram != Some(1) {
            return 0;
        }

        // The deprecated (before draft-07) way of enabling WebTransport was to send two parameters.
        // Both would send ENABLE=1 and the server would send MAX_SESSIONS=N to limit the sessions.
        // Now both just send MAX_SESSIONS, and a non-zero value means WebTransport is enabled.

        if let Some(max) = self.get(&Setting::WEBTRANSPORT_MAX_SESSIONS) {
            return max.into_inner();
        }

        let enabled = self
            .get(&Setting::WEBTRANSPORT_ENABLE_DEPRECATED)
            .map(|v| v.into_inner());
        if enabled != Some(1) {
            return 0;
        }

        // Only the server is allowed to set this one, so if it's None we assume it's 1.
        self.get(&Setting::WEBTRANSPORT_MAX_SESSIONS_DEPRECATED)
            .map(|v| v.into_inner())
            .unwrap_or(1)
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
