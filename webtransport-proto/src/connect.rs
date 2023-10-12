use std::str::FromStr;

use bytes::{Buf, BufMut};
use url::Url;

use super::{qpack, Frame, VarInt};

use thiserror::Error;

// Errors that can occur during the connect request.
#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("unexpected end of input")]
    UnexpectedEnd,

    #[error("qpack error")]
    QpackError(#[from] qpack::DecodeError),

    #[error("unexpected frame {0:?}")]
    UnexpectedFrame(Frame),

    #[error("invalid method")]
    InvalidMethod(#[from] http::method::InvalidMethod),

    #[error("invalid url")]
    InvalidUrl(#[from] url::ParseError),

    #[error("invalid status")]
    InvalidStatus(#[from] http::status::InvalidStatusCode),

    #[error("expected 200, got: {0:?}")]
    WrongStatus(Option<http::StatusCode>),

    #[error("expected connect, got: {0:?}")]
    WrongMethod(Option<http::method::Method>),

    #[error("expected https, got: {0:?}")]
    WrongScheme(Option<String>),

    #[error("expected authority header")]
    WrongAuthority,

    #[error("expected webtransport, got: {0:?}")]
    WrongProtocol(Option<String>),

    #[error("expected path header")]
    WrongPath,

    #[error("non-200 status: {0:?}")]
    ErrorStatus(http::StatusCode),
}

#[derive(Debug)]
pub struct ConnectRequest {
    pub url: Url,
}

impl ConnectRequest {
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, ConnectError> {
        let (typ, mut data) = Frame::read(buf).map_err(|_| ConnectError::UnexpectedEnd)?;
        if typ != Frame::HEADERS {
            return Err(ConnectError::UnexpectedFrame(typ));
        }

        // We no longer return UnexpectedEnd because we know the buffer should be large enough.

        let headers = qpack::Headers::decode(&mut data)?;

        let scheme = match headers.get(":scheme") {
            Some("https") => "https",
            Some(scheme) => Err(ConnectError::WrongScheme(Some(scheme.to_string())))?,
            None => return Err(ConnectError::WrongScheme(None)),
        };

        let authority = headers
            .get(":authority")
            .ok_or(ConnectError::WrongAuthority)?;

        let path = headers.get(":path").ok_or(ConnectError::WrongPath)?;

        let method = headers.get(":method");
        match method.map(|method| method.try_into()).transpose()? {
            Some(http::Method::CONNECT) => (),
            o => return Err(ConnectError::WrongMethod(o)),
        };

        let protocol = headers.get(":protocol");
        if protocol != Some("webtransport") {
            return Err(ConnectError::WrongProtocol(protocol.map(|s| s.to_string())));
        }

        let url = Url::parse(&format!("{}://{}{}", scheme, authority, path))?;

        Ok(Self { url })
    }

    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        let mut headers = qpack::Headers::default();
        headers.set(":method", "CONNECT");
        headers.set(":scheme", self.url.scheme());
        headers.set(":authority", self.url.authority());
        headers.set(":path", self.url.path());
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
        let (typ, mut data) = Frame::read(buf).map_err(|_| ConnectError::UnexpectedEnd)?;
        if typ != Frame::HEADERS {
            return Err(ConnectError::UnexpectedFrame(typ));
        }

        let headers = qpack::Headers::decode(&mut data)?;

        let status = match headers
            .get(":status")
            .map(http::StatusCode::from_str)
            .transpose()?
        {
            Some(status) if status.is_success() => status,
            o => return Err(ConnectError::WrongStatus(o)),
        };

        Ok(Self { status })
    }

    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        let mut headers = qpack::Headers::default();
        headers.set(":status", self.status.as_str());
        headers.set("sec-webtransport-http3-draft", "draft02");

        // Use a temporary buffer so we can compute the size.
        let mut tmp = Vec::new();
        headers.encode(&mut tmp);
        let size = VarInt::from_u32(tmp.len() as u32);

        Frame::HEADERS.encode(buf);
        size.encode(buf);
        buf.put_slice(&tmp);
    }
}
