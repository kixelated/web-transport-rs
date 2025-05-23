use std::sync::Arc;

use thiserror::Error;

use crate::{ConnectError, SettingsError};
use quinn::rustls;

/// An error returned when connecting to a WebTransport endpoint.
#[derive(Error, Debug, Clone)]
pub enum ClientError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("connection error: {0}")]
    Connection(#[from] quinn::ConnectionError),

    #[error("failed to write: {0}")]
    WriteError(#[from] quinn::WriteError),

    #[error("failed to read: {0}")]
    ReadError(#[from] quinn::ReadError),

    #[error("failed to exchange h3 settings: {0}")]
    SettingsError(#[from] SettingsError),

    #[error("failed to exchange h3 connect: {0}")]
    HttpError(#[from] ConnectError),

    #[error("quic error: {0}")]
    QuinnError(#[from] quinn::ConnectError),

    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),

    #[error("rustls error: {0}")]
    Rustls(#[from] rustls::Error),
}

/// An errors returned by [`crate::Session`], split based on if they are underlying QUIC errors or WebTransport errors.
#[derive(Clone, Error, Debug)]
pub enum SessionError {
    #[error("connection error: {0}")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("webtransport error: {0}")]
    WebTransportError(#[from] WebTransportError),

    #[error("send datagram error: {0}")]
    SendDatagramError(#[from] quinn::SendDatagramError),
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
    ClosedStream,
}

impl From<quinn::WriteError> for WriteError {
    fn from(e: quinn::WriteError) -> Self {
        match e {
            quinn::WriteError::Stopped(code) => {
                match web_transport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => WriteError::Stopped(code),
                    None => WriteError::InvalidStopped(code),
                }
            }
            quinn::WriteError::ClosedStream => WriteError::ClosedStream,
            quinn::WriteError::ConnectionLost(e) => WriteError::SessionError(e.into()),
            quinn::WriteError::ZeroRttRejected => unreachable!("0-RTT not supported"),
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
    ClosedStream,

    #[error("ordered read on unordered stream")]
    IllegalOrderedRead,
}

impl From<quinn::ReadError> for ReadError {
    fn from(value: quinn::ReadError) -> Self {
        match value {
            quinn::ReadError::Reset(code) => {
                match web_transport_proto::error_from_http3(code.into_inner()) {
                    Some(code) => ReadError::Reset(code),
                    None => ReadError::InvalidReset(code),
                }
            }
            quinn::ReadError::ConnectionLost(e) => ReadError::SessionError(e.into()),
            quinn::ReadError::IllegalOrderedRead => ReadError::IllegalOrderedRead,
            quinn::ReadError::ClosedStream => ReadError::ClosedStream,
            quinn::ReadError::ZeroRttRejected => unreachable!("0-RTT not supported"),
        }
    }
}

/// An error returned by [`crate::RecvStream::read_exact`]. Similar to [`quinn::ReadExactError`].
#[derive(Clone, Error, Debug)]
pub enum ReadExactError {
    #[error("finished early")]
    FinishedEarly(usize),

    #[error("read error: {0}")]
    ReadError(#[from] ReadError),
}

impl From<quinn::ReadExactError> for ReadExactError {
    fn from(e: quinn::ReadExactError) -> Self {
        match e {
            quinn::ReadExactError::FinishedEarly(size) => ReadExactError::FinishedEarly(size),
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

/// An error indicating the stream was already closed.
#[derive(Clone, Error, Debug)]
#[error("stream closed")]
pub struct ClosedStream;

impl From<quinn::ClosedStream> for ClosedStream {
    fn from(_: quinn::ClosedStream) -> Self {
        ClosedStream
    }
}

/// An error returned when receiving a new WebTransport session.
#[derive(Error, Debug, Clone)]
pub enum ServerError {
    #[error("unexpected end of stream")]
    UnexpectedEnd,

    #[error("connection error")]
    Connection(#[from] quinn::ConnectionError),

    #[error("failed to write")]
    WriteError(#[from] quinn::WriteError),

    #[error("failed to read")]
    ReadError(#[from] quinn::ReadError),

    #[error("failed to exchange h3 settings")]
    SettingsError(#[from] SettingsError),

    #[error("failed to exchange h3 connect")]
    ConnectError(#[from] ConnectError),

    #[error("io error: {0}")]
    IoError(Arc<std::io::Error>),

    #[error("rustls error: {0}")]
    Rustls(#[from] rustls::Error),
}

// #[derive(Clone, Error, Debug)]
// pub enum SendDatagramError {
//     #[error("Unsupported peer")]
//     UnsupportedPeer,

//     #[error("Datagram support Disabled by peer")]
//     DatagramSupportDisabled,

//     #[error("Datagram Too large")]
//     TooLarge,

//     #[error("Session errorr: {0}")]
//     SessionError(#[from] SessionError),
// }

// impl From<quinn::SendDatagramError> for SendDatagramError {
//     fn from(value: quinn::SendDatagramError) -> Self {
//          match value {
//              quinn::SendDatagramError::UnsupportedByPeer => SendDatagramError::UnsupportedPeer,
//              quinn::SendDatagramError::Disabled => SendDatagramError::DatagramSupportDisabled,
//              quinn::SendDatagramError::TooLarge => SendDatagramError::TooLarge,
//              quinn::SendDatagramError::ConnectionLost(e) => SendDatagramError::SessionError(e.into()),
//          }
//     }
// }
