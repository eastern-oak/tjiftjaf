// Decode fields
//
use super::PacketType;
use std::fmt::Display;

#[derive(Debug)]
pub struct InvalidPacketTypeError(pub u8);

#[derive(Debug)]
pub enum DecodingError {
    /// The bytes are not enough to decode the packet.
    NotEnoughBytes {
        minimum: usize,
        actual: usize,
    },

    /// There are too many bytes for this packet.
    TooManyBytes,

    /// The packet is not valid. Number 1 till and including 15 are valid packet numbers.
    InvalidPacketType(u8),

    InvalidValue(String),

    // The field "remaining length" is not valid.
    InvalidRemainingLength,

    // TODO: For now a 'catch-all' type. When we approach a first stable
    // release we should replace this variant with more explicit members.
    Other,
}

impl From<InvalidPacketTypeError> for DecodingError {
    fn from(value: InvalidPacketTypeError) -> Self {
        Self::InvalidPacketType(value.0)
    }
}

impl Display for DecodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::NotEnoughBytes { minimum, actual } => &format!(
                "not enough bytes available, at minimum {minimum} bytes are expected but got {actual} bytes"
            ),
            Self::TooManyBytes => "too many bytes",
            Self::InvalidPacketType(value) => &format!("{value} is not a valid packet type"),
            Self::InvalidValue(reason) => reason,
            Self::InvalidRemainingLength => &format!("Field remaining length is not valid"),
            Self::Other => &format!("Some other error"),
        };
        write!(f, "{msg}")
    }
}

pub fn packet_type(bytes: &[u8]) -> Result<PacketType, DecodingError> {
    let byte = bytes.get(0).ok_or(DecodingError::NotEnoughBytes {
        minimum: 1,
        actual: 0,
    })?;
    PacketType::try_from(byte).map_err(|_| DecodingError::InvalidPacketType(*byte))
}

pub fn packet_identifier(bytes: &[u8]) -> Result<u16, DecodingError> {
    u16(bytes)
}

pub fn u16(bytes: &[u8]) -> Result<u16, DecodingError> {
    let msb = bytes.get(0).ok_or(DecodingError::NotEnoughBytes {
        minimum: 2,
        actual: 0,
    })?;
    let lsb = bytes.get(1).ok_or(DecodingError::NotEnoughBytes {
        minimum: 2,
        actual: 0,
    })?;

    Ok(u16::from_be_bytes([*msb, *lsb]))
}

// Each packet contains a a field 'variable length'.
// This field is between 1 and 4 bytes long. The field encodes
// the number of bytes that follow _after_ the fixed header.
pub fn packet_length(bytes: &[u8]) -> Result<u32, DecodingError> {
    let mut multiplier = 1;
    let mut value: u32 = 0;
    let mut index = 0;

    loop {
        let byte = bytes.get(index).ok_or(DecodingError::NotEnoughBytes {
            minimum: index + 1,
            actual: index,
        })?;

        value += (byte & 127) as u32 * multiplier;
        multiplier *= 128;

        if byte & 128 == 0 {
            break;
        }
        index += 1;

        if index == 3 {
            return Err(DecodingError::InvalidValue(
                "The variable length field is at maximum 4 bytes long. But the third byte has the continuation bit set which indicates a fourth byte.".into(),
            ));
        }
    }
    return Ok(value + 1 + index as u32 + 1);
}

pub fn utf8(bytes: &[u8]) -> Result<&str, DecodingError> {
    let bytes = crate::decode::bytes(bytes)?;

    std::str::from_utf8(&bytes)
        .map_err(|_| DecodingError::InvalidValue("Payload is not valid UTF-8".into()))
}

pub fn bytes(bytes: &[u8]) -> Result<&[u8], DecodingError> {
    if bytes.len() < 2 {
        Err(DecodingError::NotEnoughBytes {
            minimum: 2,
            actual: bytes.len(),
        })?;
    };

    let length: usize = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    if bytes.len() < length + 2 {
        Err(DecodingError::NotEnoughBytes {
            minimum: length + 2,
            actual: bytes.len(),
        })?;
    };

    Ok(&bytes[2..2 + length])
}

pub mod field {
    use super::DecodingError;

    // Parse the next field of variable length.
    // Such field consists of a 2 byte length and the actual data.
    //
    // This is the same as calling `variable_length_n(bytes, 0)`.
    pub fn variable_length(bytes: &[u8]) -> Result<(&[u8], usize), DecodingError> {
        if bytes.len() < 2 {
            Err(DecodingError::NotEnoughBytes {
                minimum: 2,
                actual: bytes.len(),
            })?;
        };

        let length: usize = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        if bytes.len() < length + 2 {
            Err(DecodingError::NotEnoughBytes {
                minimum: length + 2,
                actual: bytes.len(),
            })?;
        };

        Ok((&bytes[2..2 + length], length + 2))
    }

    /// Parse the nth field of variable length.
    pub fn variable_length_n(bytes: &[u8], n: usize) -> Result<(&[u8], usize), DecodingError> {
        if n == 0 {
            return variable_length(bytes);
        }

        if bytes.len() < 2 {
            Err(DecodingError::NotEnoughBytes {
                minimum: 2,
                actual: bytes.len(),
            })?;
        };

        let offset: usize = u16::from_be_bytes([bytes[0], bytes[1]]) as usize + 2;
        if bytes.len() < offset {
            Err(DecodingError::NotEnoughBytes {
                minimum: offset + 2,
                actual: bytes.len(),
            })?;
        }
        variable_length_n(&bytes[offset..], n - 1)
    }
    /// Try parsing next field as `&str`.
    /// The field must starts with 2 bytes indicating the lenght of the `&str`.
    pub fn utf8(bytes: &[u8]) -> Result<(&str, usize), DecodingError> {
        let (bytes, offset) = crate::decode::field::bytes(bytes)?;

        let value = std::str::from_utf8(&bytes)
            .map_err(|_| DecodingError::InvalidValue("Payload is not valid UTF-8".into()))?;
        Ok((value, offset))
    }

    // Try parsing next field as `&[u8]`.
    // The field must starts with 2 bytes indicating the length slice.
    pub fn bytes(bytes: &[u8]) -> Result<(&[u8], usize), DecodingError> {
        if bytes.len() < 2 {
            Err(DecodingError::NotEnoughBytes {
                minimum: 2,
                actual: bytes.len(),
            })?;
        };

        let length: usize = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        if bytes.len() < length + 2 {
            Err(DecodingError::NotEnoughBytes {
                minimum: length + 2,
                actual: bytes.len(),
            })?;
        };

        Ok((&bytes[2..2 + length], length + 2))
    }
}
