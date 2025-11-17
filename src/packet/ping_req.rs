//! Providing [`PingReq`]
use crate::{Frame, Packet, decode::DecodingError};
use bytes::Bytes;

// A PINGREQ packet consists of only a header of two bytes.
// The first byte encodes the packet type, PINGREQ in this case.
// The second byte encodes the remaining length, which is 0.
const PINGREQ: [u8; 2] = [12 << 4, 0];

/// The PINGREQ Packet is sent from a Client to the Server. It can be used to:
/// * Indicate to the Server that the Client is alive in the absence of any other Control Packets being sent from the Client to the Server.
/// * Request that the Server responds to confirm that it is alive.
/// * Exercise the network to indicate that the Network Connection is active.
#[derive(Clone, Copy, PartialEq, Eq)]
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
        PingReq::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PingReq {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value == PINGREQ {
            return Ok(Self);
        }

        if value.len() < PINGREQ.len() {
            return Err(DecodingError::NotEnoughBytes {
                minimum: PINGREQ.len(),
                actual: value.len(),
            });
        }

        if value.len() > PINGREQ.len() {
            return Err(DecodingError::TooManyBytes);
        }

        Err(DecodingError::Other)
    }
}

impl From<PingReq> for Bytes {
    fn from(_: PingReq) -> Bytes {
        Bytes::copy_from_slice(&PINGREQ)
    }
}

impl From<PingReq> for Packet {
    fn from(value: PingReq) -> Packet {
        Packet::PingReq(value)
    }
}

impl std::fmt::Debug for PingReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PINGREQ")
            .field("length", &self.length())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PingReq;
    use crate::Frame;
    use bytes::Bytes;

    #[test]
    fn test_encode_and_decode() {
        // Verify conversion to and from &[u8].
        PingReq::try_from(PingReq.as_bytes()).unwrap();
        PingReq::try_from(Bytes::from(PingReq)).unwrap();

        // Verify that decoding from invalid bytes fails.
        assert!(PingReq::try_from(&[15 << 4, 0][..]).is_err());
    }

    #[test]
    fn test_variable_header() {
        // The PingReq message doesn't have a variable header.
        assert!(PingReq.variable_header().is_empty())
    }
}
