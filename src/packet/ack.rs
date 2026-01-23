//! Providing [`Ack`], a type to compose messages like [`PubAck`], [`UnsubAck`] and more.  
use crate::{decode::DecodingError, Frame, PacketType};

/// [`Ack`] is a type to compose messages like [`PubAck`], [`UnsubAck`] and a few others.  
///
/// It models is a 4 byte message.
/// * a byte that includes the packet type
/// * a byte that contains the remaining length, it's always 2.
/// * 2 bytes to encode the packet identifier.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ack([u8; 4]);

impl Ack {
    pub fn new(packet_type: PacketType, packet_identifier: u16) -> Self {
        Self([
            (packet_type as u8) << 4,
            // The remaining length,
            2,
            // The high byte of the packet identifier
            (packet_identifier >> 8) as u8,
            // The low byte of the packet identifier
            packet_identifier as u8,
        ])
    }

    pub(crate) fn packet_type(&self) -> PacketType {
        // One can only create correct instances of `Ack`, so this lookup and `unwrap()` are fine.
        PacketType::try_from(self.0[0]).unwrap()
    }

    /// Retrieve the packet identifier.
    pub(crate) fn packet_identifier(&self) -> u16 {
        // One can only create correct instances of `Ack`, so this lookups fine.
        // The last 2 bytes encode the packet identifier.
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

impl TryFrom<Vec<u8>> for Ack {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
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
        _ = PacketType::try_from(packet_type)?;

        let remaining_length = value[1];
        if remaining_length != 2 {
            return Err(DecodingError::InvalidValue(format!(
                "The remaining length must be 2, but is {remaining_length} bytes."
            )));
        }

        if value.len() > 4 {
            return Err(DecodingError::TooManyBytes);
        }

        // This unwrap is fine. We already verified that the length
        // is 4 bytes.
        Ok(Self(value.try_into().expect("Whoops! Failed to create an `Ack` because the input is not 4 bytes. Please report an issue and provide this input: {value}")))
    }
}

impl From<Ack> for Vec<u8> {
    fn from(value: Ack) -> Self {
        value.0.to_vec()
    }
}
