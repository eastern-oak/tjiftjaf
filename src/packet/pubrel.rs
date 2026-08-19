//! Providing [`PubRel`], to acknowledge a [`crate::PubRec`].
use crate::{decode::DecodingError, packet::ack::Ack, Frame, Packet, PacketType};

/// A [`PubRel`] packet is the response to a [`crate::PubRec`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubRel(Ack);

impl PubRel {
    pub fn new(packet_identifier: u16) -> Self {
        let mut ack = Ack::new(PacketType::PubRel, packet_identifier);
        // Set the second flag, as per spec.
        ack.0[0] |= 0b0010;
        Self(ack)
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for PubRel {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.0.variable_header()
    }
}

impl TryFrom<Vec<u8>> for PubRel {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        PubRel::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubRel {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() != PacketType::PubRel {
            return Err(DecodingError::InvalidPacketType(ack.packet_type() as u8));
        }

        if (value[0] & 0x0F) != 0b0010 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }
        Ok(Self(ack))
    }
}

impl From<PubRel> for Vec<u8> {
    fn from(value: PubRel) -> Vec<u8> {
        value.0.into()
    }
}

impl From<PubRel> for Packet {
    fn from(value: PubRel) -> Packet {
        Packet::PubRel(value)
    }
}

impl std::fmt::Debug for PubRel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBREL")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use crate::{DecodingError, Frame};

    use super::PubRel;

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_encode_and_decode() {
        let puback = PubRel::new(1568);
        // Verify conversion to and from &[u8].
        PubRel::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }

    /// #105 tracks a bug the parser didn't verify a message's flags.
    /// As result, the parser would happily parse a message with incorrect
    /// flags. This test verifies that the parser now fails.
    #[test]
    fn test_gh_105_fix_parsing_incorrect_flags() {
        let mut packet = PubRel::new(15).as_bytes().to_vec();
        assert!(PubRel::try_from(packet.clone()).is_ok());

        // The flags are configured in the first byte of the message.
        // This line changes the flags to an illegal value.
        packet[0] |= 0b0001;
        assert_eq!(
            PubRel::try_from(packet).unwrap_err(),
            DecodingError::HeaderContainsInvalidFlags
        );
    }
}
