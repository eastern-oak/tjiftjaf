//! Providing [`UnsubAck`], to acknowledge a [`crate::Unsubscribe`].
use crate::{decode::DecodingError, packet::ack::Ack, Frame, Packet, PacketType};

/// A [`UnsubAck`] packet is the response to a [`crate::Unsubscribe`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct UnsubAck(Ack);

impl UnsubAck {
    pub fn new(packet_identifier: u16) -> Self {
        Self(Ack::new(PacketType::UnsubAck, packet_identifier))
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for UnsubAck {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.0.variable_header()
    }
}

impl TryFrom<Vec<u8>> for UnsubAck {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        UnsubAck::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for UnsubAck {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() != PacketType::UnsubAck {
            return Err(DecodingError::InvalidPacketType(ack.packet_type() as u8));
        }

        if (value[0] & 0x0F) != 0b0000 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }
        Ok(Self(ack))
    }
}

impl From<UnsubAck> for Vec<u8> {
    fn from(value: UnsubAck) -> Vec<u8> {
        value.0.into()
    }
}

impl From<UnsubAck> for Packet {
    fn from(value: UnsubAck) -> Packet {
        Packet::UnsubAck(value)
    }
}

impl std::fmt::Debug for UnsubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UNSUBACK")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use crate::{DecodingError, Frame};

    use super::UnsubAck;

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_encode_and_decode() {
        let puback = UnsubAck::new(1568);
        // Verify conversion to and from &[u8].
        UnsubAck::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
    /// #105 tracks a bug the parser didn't verify a message's flags.
    /// As result, the parser would happily parse a message with incorrect
    /// flags. This test verifies that the parser now fails.
    #[test]
    fn test_gh_105_fix_parsing_incorrect_flags() {
        let mut packet = UnsubAck::new(15).as_bytes().to_vec();
        assert!(UnsubAck::try_from(packet.clone()).is_ok());

        // The flags are configured in the first byte of the message.
        // This line changes the flags to an illegal value.
        packet[0] |= 0b0010;
        assert_eq!(
            UnsubAck::try_from(packet).unwrap_err(),
            DecodingError::HeaderContainsInvalidFlags
        );
    }
}
