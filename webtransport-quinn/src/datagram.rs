use bytes::{Bytes, Buf};
use webtransport_proto::VarInt;
use thiserror::Error;

///HTTP3 Datagram
pub struct Datagram<B = Bytes> {
    q_stream_id: VarInt,
    payload: B,
}

impl<B> Datagram<B>
where 
    B: Buf,
{
    ///Creates a new [`Datagram`] with a given payload
    pub fn new(q_stream_id: VarInt, payload: B) -> Self {
        Datagram { 
            q_stream_id, 
            payload
        }
    }

    ///Reads a [`Datagram`] from a HTTP3 datagram
    pub fn read(mut buf: B) -> Result<Self, ErrorCode> {

        // a variable length integer that contains the value
        // of the client-initiated bidirectional stream that
        // this datagram is associated with 
        let var_int = VarInt::decode(&mut buf)
            .map_err(|_| ErrorCode::DatagramError)?;

        let q_stream_id = VarInt::from_u64(var_int.into())
            .map_err(|_| ErrorCode::DatagramError)?;

        let payload = buf;

        Ok(Self { 
            q_stream_id, 
            payload,
        })
    }

    /// Returns the associated [`QstreamId`]
    pub fn qstream_id(&self) -> VarInt {
        self.q_stream_id
    }

    /// Returns the datagram payload
    pub fn payload(&self) -> &B {
        &self.payload
    }
}

/// Error codes for [`Datagram`] operations
#[derive(Debug, Error)]
pub enum ErrorCode{
    ///HTTP3_Datagram_Error
    #[error("HTTP3_DATAGRAM Error")]
    DatagramError,
}
