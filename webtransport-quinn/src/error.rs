use std::fmt;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("connection error: {0}")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("webtransport error: {0}")]
    WebTransportError(#[from] WebTransportError),
}

#[derive(Error, Debug)]
pub enum WebTransportError {
    #[error("unknown session")]
    UnknownSession,

    #[error("read error: {0}")]
    ReadError(#[from] quinn::ReadExactError),

    #[error("write error: {0}")]
    WriteError(#[from] quinn::WriteError),
}

impl webtransport_generic::SessionError for SessionError {
    // Get the app error code from a CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32> {
        match self {
            SessionError::ConnectionError(quinn::ConnectionError::ApplicationClosed(app)) => {
                webtransport_proto::error_from_http3(app.error_code.into_inner())
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct SendError(pub quinn::WriteError);

impl webtransport_generic::SessionError for SendError {
    // Get the app error code from a CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32> {
        match &self.0 {
            quinn::WriteError::ConnectionLost(quinn::ConnectionError::ApplicationClosed(app)) => {
                webtransport_proto::error_from_http3(app.error_code.into_inner())
            }
            _ => None,
        }
    }
}

impl webtransport_generic::StreamError for SendError {
    /// Get the QUIC error code from STOP_SENDING
    fn stream_error(&self) -> Option<u32> {
        match self.0 {
            quinn::WriteError::Stopped(code) => {
                webtransport_proto::error_from_http3(code.into_inner())
            }
            _ => None,
        }
    }
}

impl From<quinn::WriteError> for SendError {
    fn from(err: quinn::WriteError) -> Self {
        Self(err)
    }
}

impl std::error::Error for SendError {}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub struct RecvError(pub quinn::ReadError);

impl webtransport_generic::SessionError for RecvError {
    // Get the app error code from a CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32> {
        match &self.0 {
            quinn::ReadError::ConnectionLost(quinn::ConnectionError::ApplicationClosed(app)) => {
                webtransport_proto::error_from_http3(app.error_code.into_inner())
            }
            _ => None,
        }
    }
}

impl webtransport_generic::StreamError for RecvError {
    /// Get the QUIC error code from STOP_SENDING
    fn stream_error(&self) -> Option<u32> {
        match self.0 {
            quinn::ReadError::Reset(code) => {
                webtransport_proto::error_from_http3(code.into_inner())
            }
            _ => None,
        }
    }
}

impl From<quinn::ReadError> for RecvError {
    fn from(err: quinn::ReadError) -> Self {
        Self(err)
    }
}

impl std::error::Error for RecvError {}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
