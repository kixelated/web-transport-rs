use std::{future::Future, ops, pin::pin};

use quinn::VarInt;

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

impl webtransport_generic::Session for Session {
    type SendStream = super::SendStream;
    type RecvStream = super::RecvStream;
    type Error = super::SessionError;

    fn poll_accept_bi(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        pin!(quinn::Connection::accept_bi(&self.0))
            .poll(cx)
            .map_ok(|(s, r)| (s.into(), r.into()))
            .map_err(Into::into)
    }

    fn poll_accept_uni(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Self::RecvStream, Self::Error>> {
        pin!(quinn::Connection::accept_uni(&self.0))
            .poll(cx)
            .map_ok(Into::into)
            .map_err(Into::into)
    }

    fn poll_open_bi(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(Self::SendStream, Self::RecvStream), Self::Error>> {
        pin!(quinn::Connection::open_bi(&self.0))
            .poll(cx)
            .map_ok(|(s, r)| (s.into(), r.into()))
            .map_err(Into::into)
    }

    fn poll_open_uni(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Self::SendStream, Self::Error>> {
        pin!(quinn::Connection::open_uni(&self.0))
            .poll(cx)
            .map_ok(Into::into)
            .map_err(Into::into)
    }

    fn close(&self, code: u32, reason: &[u8]) {
        quinn::Connection::close(self, VarInt::from_u32(code), reason)
    }

    fn poll_closed(&self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Error> {
        pin!(quinn::Connection::closed(&self.0))
            .poll(cx)
            .map(Into::into)
    }

    fn poll_recv_datagram(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<bytes::Bytes, Self::Error>> {
        pin!(quinn::Connection::read_datagram(&self.0))
            .poll(cx)
            .map_ok(Into::into)
            .map_err(Into::into)
    }

    fn send_datagram(&self, payload: bytes::Bytes) -> Result<(), Self::Error> {
        quinn::Connection::send_datagram(self, payload).map_err(|e| match e {
            quinn::SendDatagramError::ConnectionLost(err) => err.into(),
            // Not the right error but good enough
            _ => quinn::ConnectionError::Reset.into(),
        })
    }
}
