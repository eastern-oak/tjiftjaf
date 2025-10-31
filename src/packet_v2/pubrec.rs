//! Providing [`PubRec`], to acknowledge a [`super::Publish`].
use crate::packet_v2::ack::Ack;
use crate::{Frame, Packet, PacketType, decode::DecodingError};
use bytes::Bytes;

/// A [`PubRec`] packet is the response to a [`Publish`] packet with [`QoS::ExactlyOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubRec(Ack);

impl PubRec {
    pub fn new(packet_identifier: u16) -> Self {
        Self(Ack::new(PacketType::PubRec, packet_identifier))
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for PubRec {
    fn as_bytes(&self) -> &[u8] {
        &self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        &self.0.variable_header()
    }
}

impl TryFrom<Bytes> for PubRec {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        PubRec::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubRec {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() == PacketType::PubRec {
            Ok(PubRec(ack))
        } else {
            Err(DecodingError::InvalidPacketType(ack.packet_type() as u8))
        }
    }
}

impl From<PubRec> for Bytes {
    fn from(value: PubRec) -> Bytes {
        Bytes::copy_from_slice(&value.0.as_bytes())
    }
}

impl From<PubRec> for Packet {
    fn from(value: PubRec) -> Packet {
        Packet::PubRec(value)
    }
}

impl std::fmt::Debug for PubRec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubRec")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PubRec;

    #[test]
    fn test_encode_and_decode() {
        let puback = PubRec::new(1568);
        // Verify conversion to and from &[u8].
        PubRec::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
