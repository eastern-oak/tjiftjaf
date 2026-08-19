//! Providing [`Disconnect`]
use crate::{decode::DecodingError, Frame, Packet, PacketType};

// A DISCONNECT packet consists of only a header of two bytes.
// The first byte encodes the packet type, DISCONNECT in this case.
// The second byte encodes the remaining length, which is 0.
const DISCONNECT: [u8; 2] = [(PacketType::Disconnect as u8) << 4, 0];

/// The Disconnect Packet is sent from a Client to the Server.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Disconnect;

impl Frame for Disconnect {
    fn as_bytes(&self) -> &[u8] {
        &DISCONNECT
    }

    fn variable_header(&self) -> &[u8] {
        &[]
    }
}

impl TryFrom<Vec<u8>> for Disconnect {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Disconnect::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for Disconnect {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value == DISCONNECT {
            return Ok(Self);
        }

        if value.len() < DISCONNECT.len() {
            return Err(DecodingError::NotEnoughBytes {
                minimum: DISCONNECT.len(),
                actual: value.len(),
            });
        }

        if value.len() > DISCONNECT.len() {
            return Err(DecodingError::TooManyBytes);
        }

        if (value[0] & 0x0F) != 0b0000 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }

        Err(DecodingError::Other)
    }
}

impl From<Disconnect> for Vec<u8> {
    fn from(_: Disconnect) -> Self {
        DISCONNECT.to_vec()
    }
}

impl From<Disconnect> for Packet {
    fn from(value: Disconnect) -> Packet {
        Packet::Disconnect(value)
    }
}

impl std::fmt::Debug for Disconnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DISCONNECT")
            .field("length", &self.length())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::Disconnect;
    use crate::{DecodingError, Frame};

    #[test]
    fn test_encode_and_decode() {
        // Verify conversion to and from &[u8].
        Disconnect::try_from(Disconnect.as_bytes()).unwrap();
        Disconnect::try_from(Vec::from(Disconnect)).unwrap();

        // Verify that decoding from invalid bytes fails.
        assert!(Disconnect::try_from(&[15 << 4, 0][..]).is_err());
    }

    #[test]
    fn test_variable_header() {
        // The Disconnect message doesn't have a variable header.
        assert!(Disconnect.variable_header().is_empty())
    }

    /// #105 tracks a bug the parser didn't verify a message's flags.
    /// As result, the parser would happily parse a message with incorrect
    /// flags. This test verifies that the parser now fails.
    #[test]
    fn test_gh_105_fix_parsing_incorrect_flags() {
        let mut packet = Disconnect {}.as_bytes().to_vec();
        assert!(Disconnect::try_from(packet.clone()).is_ok());

        // The flags are configured in the first byte of the message.
        // This line changes the flags to an illegal value.
        packet[0] |= 0b0010;
        assert_eq!(
            Disconnect::try_from(packet).unwrap_err(),
            DecodingError::HeaderContainsInvalidFlags
        );
    }
}
