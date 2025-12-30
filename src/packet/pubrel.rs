//! Providing [`PubRel`], to acknowledge a [`crate::PubRec`].
use crate::{decode::DecodingError, Frame, Packet, PacketType};
use bytes::Bytes;

/// A [`PubRel`] packet is the response to a [`crate::PubRec`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PubRel([u8; 4]);

impl PubRel {
    pub fn new(packet_identifier: u16) -> Self {
        Self([
            // The lower nibble contains flags. The third
            // bit of this nibble is set.
            // https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718022
            ((PacketType::PubRel as u8) << 4) + 0b0010,
            // The remaining length,
            2,
            // The high byte of the packet identifier
            (packet_identifier >> 8) as u8,
            // The low byte of the packet identifier
            packet_identifier as u8,
        ])
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        // One can only create correct instances of `PubRel`, so this lookups fine.
        // The last 2 bytes encode the packet identifier.
        ((self.0[2] as u16) << 8) | self.0[3] as u16
    }
}

impl Frame for PubRel {
    fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    fn variable_header(&self) -> &[u8] {
        &self.0[2..]
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
        if value.len() < 4 {
            return Err(DecodingError::NotEnoughBytes {
                minimum: 4,
                actual: value.len(),
            });
        }

        let packet_type = value[0];
        if PacketType::try_from(packet_type)? != PacketType::PubRel {
            return Err(DecodingError::InvalidPacketType(packet_type));
        }

        if (packet_type & 0b1111) != 0b0010 {
            return Err(DecodingError::InvalidValue("Shit".into()));
        }

        let remaining_length = value[1];
        if remaining_length != 2 {
            return Err(DecodingError::InvalidValue(format!(
                "The remaining length must be 2, but is {remaining_length} bytes."
            )));
        }

        if value.len() > 4 {
            return Err(DecodingError::TooManyBytes);
        }

        Ok(Self(value.try_into().expect("Whoops! Failed to create an `Ack` because the input is not 4 bytes. Please report an issue and provide this input: {value}")))
    }
}

impl From<PubRel> for Bytes {
    fn from(value: PubRel) -> Bytes {
        Bytes::copy_from_slice(value.as_bytes())
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
    use crate::Frame;

    use super::PubRel;

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_encode_and_decode() {
        let puback = PubRel::new(1568);
        dbg!(puback.as_bytes());
        // Verify conversion to and from &[u8].
        PubRel::try_from(puback).unwrap();
        assert_eq!(puback.packet_identifier(), 1568);
    }

    // GH-104 tracks a bug where one of the flags
    // of pubrel is _not_ correctly set.
    #[test]
    fn test_fix_for_gh_104() {
        let data = [99, 2, 6, 32];
        assert!(PubRel::try_from(&data[..]).is_err());
    }
}
