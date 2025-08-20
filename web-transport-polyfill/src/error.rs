use web_transport_proto::{VarInt, VarIntUnexpectedEnd};

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("invalid frame type: {0}")]
    InvalidFrameType(u8),

    #[error("protocol violation: {0}")]
    ProtocolViolation(String),

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
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(err.to_string())
    }
}

impl web_transport_generic::Error for Error {}
