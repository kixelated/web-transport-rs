use bytes::Bytes;
use webtransport_proto::VarInt;
use thiserror::Error;

/// an HTTP/3 Datagram
/// See: <https://www.rfc-editor.org/rfc/rfc9297#section-2.1>
#[derive(Debug)]
pub struct Datagram {
    #[allow(dead_code)]
    q_stream_id: VarInt,
    payload: Bytes,
}

impl Datagram {
    ///Creates a new [`Datagram`] with a given payload
    pub fn new(q_stream_id: VarInt, payload: Bytes) -> Self {
        Datagram {
            q_stream_id,
            payload
        }
    }

    ///Reads a [`Datagram`] from a HTTP/3 datagram
    pub fn read(mut buf: Bytes) -> Result<Self, DatagramError> {
        // a variable length integer that contains the value
        // of the client-initiated bidirectional stream that
        // this datagram is associated with
        let q_stream_id = VarInt::decode(&mut buf)
            .map_err(|_| DatagramError::InvalidQStreamId)?;

        let datagram = Self {
            q_stream_id,
            payload: buf.clone(),
        };

        Ok(datagram)
    }

    /// Returns the datagram payload
    pub fn payload(&self) -> &Bytes {
        &self.payload
    }

}

#[derive(Debug, Error)]
pub enum DatagramError {
     ///HTTP/3_Datagram_Error
     #[error("HTTP3_DATAGRAM Error")]
     InvalidQStreamId,
}
