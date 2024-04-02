use std::ops;

use bytes::Bytes;

use crate::{RecvStream, SendStream, SessionError};

#[derive(Clone)]
pub struct Session(quinn::Connection);

impl ops::Deref for Session {
    type Target = quinn::Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<quinn::Connection> for Session {
    fn from(connection: quinn::Connection) -> Self {
        Session(connection)
    }
}

#[async_trait::async_trait(?Send)]
impl webtransport_generic::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = SessionError;

    /// Accept an incoming unidirectional stream
    async fn accept_uni(&mut self) -> Result<Self::RecvStream, Self::Error> {
        Ok(quinn::Connection::accept_uni(self).await?.into())
    }

    /// Accept an incoming bidirectional stream
    async fn accept_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        let pair = quinn::Connection::accept_bi(self).await?;
        Ok((pair.0.into(), pair.1.into()))
    }

    async fn open_uni(&mut self) -> Result<Self::SendStream, Self::Error> {
        Ok(quinn::Connection::open_uni(self).await?.into())
    }

    async fn open_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        let pair = quinn::Connection::open_bi(self).await?;
        Ok((pair.0.into(), pair.1.into()))
    }

    /// Close the connection immediately
    fn close(self, code: u32, reason: &str) {
        quinn::Connection::close(&self, code.into(), reason.as_bytes())
    }

    async fn closed(&self) -> Self::Error {
        quinn::Connection::closed(self).await.into()
    }

    async fn recv_datagram(&mut self) -> Result<Bytes, Self::Error> {
        Ok(quinn::Connection::read_datagram(self).await?.into())
    }

    async fn send_datagram(&mut self, data: Bytes) -> Result<(), Self::Error> {
        quinn::Connection::send_datagram(self, data)
            .map_err(|err| match err {
                quinn::SendDatagramError::ConnectionLost(err) => err,
                _ => quinn::ConnectionError::Reset, // TODO: better error
            })
            .map_err(Into::into)
    }
}
