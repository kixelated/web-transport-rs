use web_transport_proto::{VarInt, VarIntUnexpectedEnd};

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("invalid frame type: {0}")]
    InvalidFrameType(u8),

    #[error("text messages not allowed")]
    NoText,

    #[error("pong messages not allowed")]
    NoPong,

    #[error("generic frames not allowed")]
    NoGenericFrames,

    #[error("invalid stream id")]
    InvalidStreamId,

    #[error("stream closed")]
    StreamClosed,

    #[error("connection closed: {code}: {reason}")]
    ConnectionClosed { code: VarInt, reason: String },

    #[error("stream reset: {0}")]
    StreamReset(VarInt),

    #[error("stream stop: {0}")]
    StreamStop(VarInt),

    #[error("short frame")]
    Short,

    #[error("connection closed")]
    Closed,
}

impl From<VarIntUnexpectedEnd> for Error {
    fn from(_: VarIntUnexpectedEnd) -> Self {
        Self::Short
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    fn from(_err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Closed
    }
}

impl web_transport_trait::Error for Error {}
