use std::io;

use webtransport_proto::{ConnectRequest, ConnectResponse, VarInt};

use thiserror::Error;
use url::Url;

#[derive(Error, Debug, Clone)]
pub enum ConnectError {
    #[error("quic stream was closed early")]
    UnexpectedEnd,

    #[error("protocol error: {0}")]
    ProtoError(#[from] webtransport_proto::ConnectError),

    #[error("connection error")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("read error")]
    ReadError(#[from] quinn::ReadError),

    #[error("write error")]
    WriteError(#[from] quinn::WriteError),

    #[error("http error status: {0}")]
    ErrorStatus(http::StatusCode),
}

pub struct Connect {
    // The request that was sent by the client.
    request: ConnectRequest,

    // A reference to the send/recv stream, so we don't close it until dropped.
    send: quinn::SendStream,

    #[allow(dead_code)]
    recv: quinn::RecvStream,
}

impl Connect {
    pub async fn accept(conn: &quinn::Connection) -> Result<Self, ConnectError> {
        // Accept the stream that will be used to send the HTTP CONNECT request.
        // If they try to send any other type of HTTP request, we will error out.
        let (send, mut recv) = conn.accept_bi().await?;
        let mut buf = Vec::new();

        // Read the request from the client, buffering more data until we get a full response.
        loop {
            // Read more data into the buffer.
            // We use the chunk API here instead of read_buf literally just to return a quinn::ReadError instead of io::Error.
            let chunk = recv.read_chunk(usize::MAX, true).await?;
            let chunk = chunk.ok_or(ConnectError::UnexpectedEnd)?;
            buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

            // Create a cursor that will tell us how much of the buffer was read.
            let mut limit = io::Cursor::new(&buf);

            // Try to decode the request.
            let request = match ConnectRequest::decode(&mut limit) {
                // It worked, return it.
                Ok(req) => req,

                // We didn't have enough data in the buffer, so we'll read more and try again.
                Err(webtransport_proto::ConnectError::UnexpectedEnd) => {
                    log::debug!("buffering CONNECT request");
                    continue;
                }

                // Some other fatal error.
                Err(e) => return Err(e.into()),
            };

            log::debug!("received CONNECT request: {:?}", request);

            // The request was successfully decoded, so we can send a response.
            return Ok(Self {
                request,
                send,
                recv,
            });
        }
    }

    // Called by the server to send a response to the client.
    pub async fn respond(&mut self, status: http::StatusCode) -> Result<(), quinn::WriteError> {
        let resp = ConnectResponse { status };

        log::debug!("sending CONNECT response: {:?}", resp);

        let mut buf = Vec::new();
        resp.encode(&mut buf);

        self.send.write_all(&buf).await?;

        Ok(())
    }

    pub async fn open(conn: &quinn::Connection, url: &Url) -> Result<Self, ConnectError> {
        // Create a new stream that will be used to send the CONNECT frame.
        let (mut send, mut recv) = conn.open_bi().await?;

        // Create a new CONNECT request that we'll send using HTTP/3
        let request = ConnectRequest { url: url.clone() };

        log::debug!("sending CONNECT request: {:?}", request);

        // Encode our connect request into a buffer and write it to the stream.
        let mut buf = Vec::new();
        request.encode(&mut buf);
        send.write_all(&buf).await?;

        buf.clear();

        // Read the response from the server, buffering more data until we get a full response.
        loop {
            // Read more data into the buffer.
            // We use the chunk API here instead of read_buf literally just to return a quinn::ReadError instead of io::Error.
            let chunk = recv.read_chunk(usize::MAX, true).await?;
            let chunk = chunk.ok_or(ConnectError::UnexpectedEnd)?;
            buf.extend_from_slice(&chunk.bytes); // TODO avoid copying on the first loop.

            // Create a cursor that will tell us how much of the buffer was read.
            let mut limit = io::Cursor::new(&buf);

            // Try to decode the response.
            let res = match ConnectResponse::decode(&mut limit) {
                // It worked, return it.
                Ok(res) => res,

                // We didn't have enough data in the buffer, so we'll read more and try again.
                Err(webtransport_proto::ConnectError::UnexpectedEnd) => {
                    log::debug!("buffering CONNECT response");
                    continue;
                }

                // Some other fatal error.
                Err(e) => return Err(e.into()),
            };

            log::debug!("received CONNECT response: {:?}", res);

            // Throw an error if we didn't get a 200 OK.
            if res.status != http::StatusCode::OK {
                return Err(ConnectError::ErrorStatus(res.status));
            }

            return Ok(Self {
                request,
                send,
                recv,
            });
        }
    }

    // The session ID is the stream ID of the CONNECT request.
    pub fn session_id(&self) -> VarInt {
        // We gotta convert from the Quinn VarInt to the (forked) WebTransport VarInt.
        // We don't use the quinn::VarInt because that would mean a quinn dependency in webtransport-proto
        let stream_id = quinn::VarInt::from(self.send.id());
        VarInt::try_from(stream_id.into_inner()).unwrap()
    }

    // The URL in the CONNECT request.
    pub fn url(&self) -> &Url {
        &self.request.url
    }
}
