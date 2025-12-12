//! Providing [`Disconnect`]
use crate::{decode::DecodingError, Frame, Packet, PacketType};
use bytes::Bytes;

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

impl TryFrom<Bytes> for Disconnect {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
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

        Err(DecodingError::Other)
    }
}

impl From<Disconnect> for Bytes {
    fn from(_: Disconnect) -> Bytes {
        Bytes::copy_from_slice(&DISCONNECT)
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
    use crate::Frame;
    use bytes::Bytes;

    #[test]
    fn test_encode_and_decode() {
        // Verify conversion to and from &[u8].
        Disconnect::try_from(Disconnect.as_bytes()).unwrap();
        Disconnect::try_from(Bytes::from(Disconnect)).unwrap();

        // Verify that decoding from invalid bytes fails.
        assert!(Disconnect::try_from(&[15 << 4, 0][..]).is_err());
    }

    #[test]
    fn test_variable_header() {
        // The Disconnect message doesn't have a variable header.
        assert!(Disconnect.variable_header().is_empty())
    }
}
