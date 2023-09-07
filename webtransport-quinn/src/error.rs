use thiserror::Error;

/// An errors returned by [`crate::Session`], split based on if they are underlying QUIC errors or WebTransport errors.
#[derive(Clone, Error, Debug)]
pub enum SessionError {
    #[error("connection error: {0}")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("webtransport error: {0}")]
    WebTransportError(#[from] WebTransportError),
}

/// An error that can occur when reading/writing the WebTransport stream header.
#[derive(Clone, Error, Debug)]
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

/// An error when writing to [`crate::SendStream`]. Similar to [`quinn::WriteError`].
#[derive(Clone, Error, Debug)]
pub enum WriteError {
    #[error("STOP_SENDING: {0}")]
    Stopped(u32),

    #[error("invalid STOP_SENDING: {0}")]
    InvalidStopped(quinn::VarInt),

    #[error("session error: {0}")]
    SessionError(#[from] SessionError),

    #[error("stream closed")]
    Closed,
}

impl From<quinn::WriteError> for WriteError {
    fn from(e: quinn::WriteError) -> Self {
        match e {
            quinn::WriteError::Stopped(code) => {
                match webtransport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => WriteError::Stopped(code),
                    None => WriteError::InvalidStopped(code),
                }
            }
            quinn::WriteError::UnknownStream => WriteError::Closed,
            quinn::WriteError::ConnectionLost(e) => WriteError::SessionError(e.into()),
            quinn::WriteError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}

impl webtransport_generic::SessionError for WriteError {
    // Get the app error code from a CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32> {
        match self {
            WriteError::SessionError(e) => e.session_error(),
            _ => None,
        }
    }
}

/// An error when reading from [`crate::RecvStream`]. Similar to [`quinn::ReadError`].
#[derive(Clone, Error, Debug)]
pub enum ReadError {
    #[error("session error: {0}")]
    SessionError(#[from] SessionError),

    #[error("RESET_STREAM: {0}")]
    Reset(u32),

    #[error("invalid RESET_STREAM: {0}")]
    InvalidReset(quinn::VarInt),

    #[error("stream already closed")]
    Closed,

    #[error("ordered read on unordered stream")]
    IllegalOrderedRead,
}

impl From<quinn::ReadError> for ReadError {
    fn from(value: quinn::ReadError) -> Self {
        match value {
            quinn::ReadError::Reset(code) => {
                match webtransport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => ReadError::Reset(code),
                    None => ReadError::InvalidReset(code),
                }
            }
            quinn::ReadError::ConnectionLost(e) => ReadError::SessionError(e.into()),
            quinn::ReadError::IllegalOrderedRead => ReadError::IllegalOrderedRead,
            quinn::ReadError::UnknownStream => ReadError::Closed,
            quinn::ReadError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}

impl webtransport_generic::SessionError for ReadError {
    // Get the app error code from a CONNECTION_CLOSE
    fn session_error(&self) -> Option<u32> {
        match self {
            ReadError::SessionError(e) => e.session_error(),
            _ => None,
        }
    }
}

/// An error returned by [`crate::RecvStream::read_exact`]. Similar to [`quinn::ReadExactError`].
#[derive(Clone, Error, Debug)]
pub enum ReadExactError {
    #[error("finished early")]
    FinishedEarly,

    #[error("read error: {0}")]
    ReadError(#[from] ReadError),
}

impl From<quinn::ReadExactError> for ReadExactError {
    fn from(e: quinn::ReadExactError) -> Self {
        match e {
            quinn::ReadExactError::FinishedEarly => ReadExactError::FinishedEarly,
            quinn::ReadExactError::ReadError(e) => ReadExactError::ReadError(e.into()),
        }
    }
}

/// An error returned by [`crate::RecvStream::read_to_end`]. Similar to [`quinn::ReadToEndError`].
#[derive(Clone, Error, Debug)]
pub enum ReadToEndError {
    #[error("too long")]
    TooLong,

    #[error("read error: {0}")]
    ReadError(#[from] ReadError),
}

impl From<quinn::ReadToEndError> for ReadToEndError {
    fn from(e: quinn::ReadToEndError) -> Self {
        match e {
            quinn::ReadToEndError::TooLong => ReadToEndError::TooLong,
            quinn::ReadToEndError::Read(e) => ReadToEndError::ReadError(e.into()),
        }
    }
}

/// An error indicating the stream was already closed. Same as [`quinn::UnknownStream`] but a less confusing name.
#[derive(Clone, Error, Debug)]
#[error("stream closed")]
pub struct StreamClosed;

impl From<quinn::UnknownStream> for StreamClosed {
    fn from(_: quinn::UnknownStream) -> Self {
        StreamClosed
    }
}

/// An error returned by [`crate::SendStream::stopped`]. Similar to [`quinn::StoppedError`].
#[derive(Clone, Error, Debug)]
pub enum StoppedError {
    #[error("session error: {0}")]
    SessionError(#[from] SessionError),

    #[error("stream already closed")]
    Closed,
}

impl From<quinn::StoppedError> for StoppedError {
    fn from(e: quinn::StoppedError) -> Self {
        match e {
            quinn::StoppedError::ConnectionLost(e) => StoppedError::SessionError(e.into()),
            quinn::StoppedError::UnknownStream => StoppedError::Closed,
            quinn::StoppedError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}
