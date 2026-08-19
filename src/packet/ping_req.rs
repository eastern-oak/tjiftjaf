//! Providing [`PingReq`]
use crate::{decode::DecodingError, Frame, Packet};

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

impl TryFrom<Vec<u8>> for PingReq {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
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

        if (value[0] & 0x0F) != 0b0000 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }

        Err(DecodingError::Other)
    }
}

impl From<PingReq> for Vec<u8> {
    fn from(_: PingReq) -> Self {
        PINGREQ.to_vec()
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
    use crate::{DecodingError, Frame};

    #[test]
    fn test_encode_and_decode() {
        // Verify conversion to and from &[u8].
        PingReq::try_from(PingReq.as_bytes()).unwrap();
        PingReq::try_from(Vec::from(PingReq)).unwrap();

        // Verify that decoding from invalid bytes fails.
        assert!(PingReq::try_from(&[15 << 4, 0][..]).is_err());
    }

    #[test]
    fn test_variable_header() {
        // The PingReq message doesn't have a variable header.
        assert!(PingReq.variable_header().is_empty())
    }

    /// #105 tracks a bug the parser didn't verify a message's flags.
    /// As result, the parser would happily parse a message with incorrect
    /// flags. This test verifies that the parser now fails.
    #[test]
    fn test_gh_105_fix_parsing_incorrect_flags() {
        let mut packet = PingReq {}.as_bytes().to_vec();
        assert!(PingReq::try_from(packet.clone()).is_ok());

        // The flags are configured in the first byte of the message.
        // This line changes the flags to an illegal value.
        packet[0] |= 0b0010;
        assert_eq!(
            PingReq::try_from(packet).unwrap_err(),
            DecodingError::HeaderContainsInvalidFlags
        );
    }
}
