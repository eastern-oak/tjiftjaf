//! Providing [`PingResp`]
use crate::{Frame, decode::DecodingError};
use bytes::Bytes;

// A PINGRESP packet consists of only a header of two bytes.
// The first byte encodes the packet type, PINGRESP in this case.
// The second byte encodes the remaining length, which is 0.
const PINGRESP: [u8; 2] = [13 << 4, 0];

/// A PINGRESP Packet is sent by the Server to the Client in response to a PINGREQ Packet. It indicates that the Server is alive.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PingResp;

impl Frame for PingResp {
    fn as_bytes(&self) -> &[u8] {
        &PINGRESP
    }

    fn variable_header(&self) -> &[u8] {
        &[]
    }
}

impl TryFrom<Bytes> for PingResp {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        PingResp::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PingResp {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value == PINGRESP {
            return Ok(Self);
        }

        if value.len() < PINGRESP.len() {
            return Err(DecodingError::NotEnoughBytes {
                minimum: PINGRESP.len(),
                actual: value.len(),
            });
        }

        if value.len() > PINGRESP.len() {
            return Err(DecodingError::TooManyBytes);
        }

        Err(DecodingError::Other)
    }
}

impl From<PingResp> for Bytes {
    fn from(_: PingResp) -> Bytes {
        Bytes::copy_from_slice(&PINGRESP)
    }
}

impl std::fmt::Debug for PingResp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PINGRESP")
            .field("length", &self.length())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PingResp;
    use crate::Frame;
    use bytes::Bytes;

    #[test]
    fn test_encode_and_decode() {
        // Verify conversion to and from &[u8].
        PingResp::try_from(PingResp.as_bytes()).unwrap();
        PingResp::try_from(Bytes::from(PingResp)).unwrap();

        // Verify that decoding from invalid bytes fails.
        assert!(PingResp::try_from(&[15 << 4, 0][..]).is_err());
    }

    #[test]
    fn test_variable_header() {
        // The PingResp message doesn't have a variable header.
        assert!(PingResp.variable_header().is_empty())
    }
}
