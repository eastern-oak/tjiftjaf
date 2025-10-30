//! Providing [`PubAck`], to acknowledge a [`super::Publish`].
use crate::{Frame, Packet, PacketType, decode::DecodingError};
use bytes::Bytes;

/// A [`PubAck`] packet is the response to a [`Publish`] packet with [`QoS::AtLeastOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubAck([u8; 4]);

impl PubAck {
    pub fn new(packet_identifier: u16) -> Self {
        Self([
            (PacketType::PubAck as u8) << 4,
            2, // The remaining length,
            (packet_identifier >> 8) as u8,
            packet_identifier as u8,
        ])
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        ((self.0[2] as u16) << 8) | self.0[3] as u16
    }
}

impl Frame for PubAck {
    fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    fn variable_header(&self) -> &[u8] {
        &self.0[2..]
    }
}

impl TryFrom<Bytes> for PubAck {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        PubAck::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for PubAck {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 4 {
            return Err(DecodingError::NotEnoughBytes {
                minimum: 4,
                actual: value.len(),
            });
        }

        let packet_type = value[0];
        if packet_type != (PacketType::PubAck as u8) << 4 {
            return Err(DecodingError::InvalidPacketType(packet_type));
        }

        let remaining_length = value[1];
        if remaining_length != 2 {
            return Err(DecodingError::InvalidValue(format!("Length must be 2")));
        }

        if value.len() > 4 {
            return Err(DecodingError::TooManyBytes);
        }

        Ok(Self([packet_type, remaining_length, value[2], value[3]]))
    }
}

impl From<PubAck> for Bytes {
    fn from(value: PubAck) -> Bytes {
        Bytes::copy_from_slice(&value.0)
    }
}

impl From<PubAck> for Packet {
    fn from(value: PubAck) -> Packet {
        Packet::PubAck(value)
    }
}

impl std::fmt::Debug for PubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubAck")
            .field("length", &self.length())
            .field("packet_identifier", &self.length())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::PubAck;

    #[test]
    fn test_encode_and_decode() {
        let puback = PubAck::new(1568);
        // Verify conversion to and from &[u8].
        PubAck::try_from(puback).unwrap();

        assert_eq!(puback.packet_identifier(), 1568);
    }
}
