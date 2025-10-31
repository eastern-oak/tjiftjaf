//! Providing [`Ack`], a type to compose messages like [`PubAck`], [`UnsubAck`] and more.  
use crate::{Frame, PacketType, decode::DecodingError};
use bytes::Bytes;

/// A [`Ack`] packet is the response to a [`Publish`] packet with [`QoS::AtLeastOnceDelivery`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ack([u8; 4]);

impl Ack {
    pub fn new(packet_type: PacketType, packet_identifier: u16) -> Self {
        Self([
            (packet_type as u8) << 4,
            2, // The remaining length,
            (packet_identifier >> 8) as u8,
            packet_identifier as u8,
        ])
    }

    /// Retrieve the packet identifier.
    pub(crate) fn packet_identifier(&self) -> u16 {
        ((self.0[2] as u16) << 8) | self.0[3] as u16
    }
}

impl Frame for Ack {
    fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    fn variable_header(&self) -> &[u8] {
        &self.0[2..]
    }
}

impl TryFrom<Bytes> for Ack {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        Ack::try_from(value.as_ref())
    }
}

impl TryFrom<&[u8]> for Ack {
    type Error = DecodingError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < 4 {
            return Err(DecodingError::NotEnoughBytes {
                minimum: 4,
                actual: value.len(),
            });
        }

        let packet_type = value[0];
        _ = PacketType::try_from(packet_type).unwrap();
        // .map_err(|err| Err(DecodingError::InvalidPacketType(packet_type)))?;

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
