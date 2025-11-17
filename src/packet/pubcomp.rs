//! Providing [`PubComp`], a messages that acknowledges a [`super::PubRel`].
use crate::{Frame, Packet, PacketType, decode::DecodingError, packet::ack::Ack};
use bytes::Bytes;

///[`PubComp`] is the response to a [`super::PubRel`] packet with [`QoS::OnlyOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubComp(Ack);

impl PubComp {
    pub fn new(packet_identifier: u16) -> Self {
        Self(Ack::new(PacketType::PubComp, packet_identifier))
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.0.packet_identifier()
    }
}

impl Frame for PubComp {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.0.variable_header()
    }
}

impl TryFrom<Bytes> for PubComp {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        PubComp::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubComp {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() == PacketType::PubComp {
            Ok(PubComp(ack))
        } else {
            Err(DecodingError::InvalidPacketType(ack.packet_type() as u8))
        }
    }
}

impl From<PubComp> for Bytes {
    fn from(value: PubComp) -> Bytes {
        Bytes::copy_from_slice(value.0.as_bytes())
    }
}

impl From<PubComp> for Packet {
    fn from(value: PubComp) -> Packet {
        Packet::PubComp(value)
    }
}

impl std::fmt::Debug for PubComp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBCOMP")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PubComp;

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_encode_and_decode() {
        let puback = PubComp::new(1568);
        // Verify conversion to and from &[u8].
        PubComp::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
