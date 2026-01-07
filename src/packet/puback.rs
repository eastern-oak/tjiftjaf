//! Providing [`PubAck`], to acknowledge a [`crate::Publish`].
use crate::{decode::DecodingError, packet::ack::Ack, Frame, Packet, PacketType};

/// A [`PubAck`] packet is the response to a [`crate::Publish`] packet with
/// [`crate::QoS::AtLeastOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubAck(Ack);

impl PubAck {
    /// Create a new `PubAck` with the given packet identifier.
    pub fn new(packet_identifier: u16) -> Self {
        Self(Ack::new(PacketType::PubAck, packet_identifier))
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for PubAck {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.0.variable_header()
    }
}

impl TryFrom<Vec<u8>> for PubAck {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        PubAck::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubAck {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() == PacketType::PubAck {
            Ok(PubAck(ack))
        } else {
            Err(DecodingError::InvalidPacketType(ack.packet_type() as u8))
        }
    }
}

impl From<PubAck> for Vec<u8> {
    fn from(value: PubAck) -> Vec<u8> {
        value.0.into()
    }
}

impl From<PubAck> for Packet {
    fn from(value: PubAck) -> Packet {
        Packet::PubAck(value)
    }
}

impl std::fmt::Debug for PubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBACK")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PubAck;

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_encode_and_decode() {
        let puback = PubAck::new(1568);
        // Verify conversion to and from &[u8].
        PubAck::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
