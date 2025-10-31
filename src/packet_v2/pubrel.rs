//! Providing [`PubRel`], to acknowledge a [`super::PubComp`].
use crate::packet_v2::ack::Ack;
use crate::{Frame, Packet, PacketType, decode::DecodingError};
use bytes::Bytes;

/// A [`PubRel`] packet is the response to a [`Publish`] packet with [`QoS::ExactlyOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubRel(Ack);

impl PubRel {
    pub fn new(packet_identifier: u16) -> Self {
        Self(Ack::new(PacketType::PubRel, packet_identifier))
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for PubRel {
    fn as_bytes(&self) -> &[u8] {
        &self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        &self.0.variable_header()
    }
}

impl TryFrom<Bytes> for PubRel {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        PubRel::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubRel {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() == PacketType::PubRel {
            Ok(PubRel(ack))
        } else {
            Err(DecodingError::InvalidPacketType(ack.packet_type() as u8))
        }
    }
}

impl From<PubRel> for Bytes {
    fn from(value: PubRel) -> Bytes {
        Bytes::copy_from_slice(&value.0.as_bytes())
    }
}

impl From<PubRel> for Packet {
    fn from(value: PubRel) -> Packet {
        Packet::PubRel(value)
    }
}

impl std::fmt::Debug for PubRel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubRel")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PubRel;

    #[test]
    fn test_encode_and_decode() {
        let puback = PubRel::new(1568);
        // Verify conversion to and from &[u8].
        PubRel::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
