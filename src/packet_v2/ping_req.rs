//! Providing [`PingReq`]
use crate::{Frame, decode::DecodingError};
use bytes::Bytes;

// A PINGREQ packet consists of only a header of two bytes.
// The first byte encodes the packet type, PINGREQ in this case.
// The second byte encodes the remaining length, which is 0.
const PINGREQ: [u8; 2] = [12 << 4, 0];

/// The INGREQ Packet is sent from a Client to the Server. It can be used to:
/// * Indicate to the Server that the Client is alive in the absence of any other Control Packets being sent from the Client to the Server.
/// * Request that the Server responds to confirm that it is alive.
/// * Exercise the network to indicate that the Network Connection is active.
#[derive(Clone, PartialEq)]
pub struct PingReq;

impl Frame for PingReq {
    fn as_bytes(&self) -> &[u8] {
        &PINGREQ
    }

    fn variable_header(&self) -> &[u8] {
        &[]
    }
}

impl TryFrom<Bytes> for PingReq {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        if value == PINGREQ[..] {
            return Ok(Self);
        }

        if value.len() < PINGREQ.len() {
            return Err(DecodingError::NotEnoughBytes {
                minimum: PINGREQ.len() as usize,
                actual: value.len(),
            });
        }

        if value.len() > PINGREQ.len() {
            return Err(DecodingError::TooManyBytes);
        }

        Err(DecodingError::Other)
    }
}

impl Into<Bytes> for PingReq {
    fn into(self) -> Bytes {
        Bytes::copy_from_slice(&PINGREQ)
    }
}

impl std::fmt::Debug for PingReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PINGREQ")
            .field("length", &self.length())
            .finish()
    }
}
