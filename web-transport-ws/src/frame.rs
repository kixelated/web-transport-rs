use bytes::{Buf, BufMut, Bytes, BytesMut};
use web_transport_proto::VarInt;

use crate::{Error, StreamId};

/// QUIC Frame types (subset for WebSocket transport)
const RESET_STREAM: u8 = 0x04;
const STOP_SENDING: u8 = 0x05;
const STREAM: u8 = 0x08;
const STREAM_FIN: u8 = 0x09;
const APPLICATION_CLOSE: u8 = 0x1d;

#[derive(Debug, Clone)]
pub struct Stream {
    pub id: StreamId,
    pub data: Bytes,
    // no offset, because everything is ordered
    // no length, because WebSocket already provides this
    pub fin: bool,
}

impl Stream {
    pub fn encode(&self, mut buf: &mut BytesMut) {
        // Calculate frame type based on flags
        match self.fin {
            true => buf.put_u8(STREAM_FIN),
            false => buf.put_u8(STREAM),
        };

        self.id.0.encode(&mut buf);
        buf.put_slice(&self.data);
    }

    pub fn decode(mut data: Bytes, fin: bool) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);
        Ok(Stream { id, data, fin })
    }
}

#[derive(Debug, Clone)]
pub struct ResetStream {
    pub id: StreamId,
    pub code: VarInt,
    // no final size, because there's no flow control
}

impl ResetStream {
    pub fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(RESET_STREAM);
        self.id.0.encode(&mut buf);
        self.code.encode(&mut buf);
    }

    pub fn decode(mut data: Bytes) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);
        let code = VarInt::decode(&mut data)?;
        Ok(ResetStream { id, code })
    }
}

#[derive(Debug, Clone)]
pub struct StopSending {
    pub id: StreamId,
    pub code: VarInt,
}

impl StopSending {
    pub fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(STOP_SENDING);
        self.id.0.encode(&mut buf);
        self.code.encode(&mut buf);
    }

    pub fn decode(mut data: Bytes) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);
        let code = VarInt::decode(&mut data)?;
        Ok(StopSending { id, code })
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionClose {
    pub code: VarInt,
    // no reason size, because WebSocket already provides this.
    pub reason: String,
}

impl ConnectionClose {
    pub fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(APPLICATION_CLOSE);
        self.code.encode(&mut buf);
        buf.put_slice(self.reason.as_bytes());
    }

    pub fn decode(mut data: Bytes) -> Result<Self, Error> {
        let code = VarInt::decode(&mut data)?;
        let reason = String::from_utf8_lossy(&data).into_owned();
        Ok(ConnectionClose { code, reason })
    }
}

/// QUIC-compatible frames for WebSocket transport
#[derive(Debug)]
pub enum Frame {
    ResetStream(ResetStream),
    StopSending(StopSending),
    ConnectionClose(ConnectionClose),
    Stream(Stream),
}

impl Frame {
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Frame::ResetStream(frame) => frame.encode(&mut buf),
            Frame::StopSending(frame) => frame.encode(&mut buf),
            Frame::Stream(frame) => frame.encode(&mut buf),
            Frame::ConnectionClose(frame) => frame.encode(&mut buf),
        }

        buf.freeze()
    }

    pub fn decode(mut data: Bytes) -> Result<Self, Error> {
        if data.is_empty() {
            return Err(Error::Short);
        }

        let frame_type = data.get_u8();

        match frame_type {
            RESET_STREAM => Ok(Frame::ResetStream(ResetStream::decode(data)?)),
            STOP_SENDING => Ok(Frame::StopSending(StopSending::decode(data)?)),
            STREAM => Ok(Frame::Stream(Stream::decode(data, false)?)),
            STREAM_FIN => Ok(Frame::Stream(Stream::decode(data, true)?)),
            APPLICATION_CLOSE => Ok(Frame::ConnectionClose(ConnectionClose::decode(data)?)),
            _ => Err(Error::InvalidFrameType(frame_type)),
        }
    }
}

impl From<Stream> for Frame {
    fn from(stream: Stream) -> Self {
        Frame::Stream(stream)
    }
}

impl From<ResetStream> for Frame {
    fn from(reset: ResetStream) -> Self {
        Frame::ResetStream(reset)
    }
}

impl From<StopSending> for Frame {
    fn from(stop: StopSending) -> Self {
        Frame::StopSending(stop)
    }
}

impl From<ConnectionClose> for Frame {
    fn from(close: ConnectionClose) -> Self {
        Frame::ConnectionClose(close)
    }
}
