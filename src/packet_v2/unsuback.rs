//! Providing [`UnsubAck`], to acknowledge a [`super::Unsubscribe`].
use crate::packet_v2::ack::Ack;
use crate::{Frame, Packet, PacketType, decode::DecodingError};
use bytes::Bytes;

/// A [`UnsubAck`] packet is the response to a [`Unsubscribe`].
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
        &self.0.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        &self.0.variable_header()
    }
}

impl TryFrom<Bytes> for UnsubAck {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        UnsubAck::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for UnsubAck {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let ack = Ack::try_from(value)?;
        if ack.packet_type() == PacketType::UnsubAck {
            Ok(UnsubAck(ack))
        } else {
            Err(DecodingError::InvalidPacketType(ack.packet_type() as u8))
        }
    }
}

impl From<UnsubAck> for Bytes {
    fn from(value: UnsubAck) -> Bytes {
        Bytes::copy_from_slice(&value.0.as_bytes())
    }
}

impl From<UnsubAck> for Packet {
    fn from(value: UnsubAck) -> Packet {
        Packet::UnsubAck(value)
    }
}

impl std::fmt::Debug for UnsubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnsubAck")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::UnsubAck;

    #[test]
    fn test_encode_and_decode() {
        let puback = UnsubAck::new(1568);
        // Verify conversion to and from &[u8].
        UnsubAck::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
