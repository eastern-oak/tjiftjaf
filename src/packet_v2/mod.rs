use crate::decode::DecodingError;
mod ack;
pub mod connack;
pub mod connect;
pub mod ping_req;
pub mod ping_resp;
pub mod puback;
pub mod publish;
pub mod pubrec;
pub mod suback;
pub mod subscribe;
pub mod unsuback;

// Retrieve the fixed header, variable header, and payload a frame.
// Since the frame is not verified (yet), these operations are fallible.
pub trait UnverifiedFrame {
    fn as_bytes(&self) -> &[u8];

    // Return the actual length of the frame.
    fn length(&self) -> usize {
        self.as_bytes().len()
    }

    /// Return a slice containing the fixed header.
    fn try_header(&self) -> Result<&[u8], DecodingError> {
        let inner = self.as_bytes();

        // Decode the "remaining length" field. This field is between
        // 1 and 3 bytes long and contains the number that follow _after_
        // this field.
        //
        // The length of the entire packet is:
        // * the value encoded is this field
        // * the length of this field (between 1 and 3 bytes)
        // * 1 byte for encoding the packet type
        for n in 1..4 {
            let byte = inner.get(n).ok_or(DecodingError::NotEnoughBytes {
                minimum: 1,
                actual: 0,
            })?;

            if byte & 128 == 0 {
                // TODO: Make lookup infallible.
                return Ok(&self.as_bytes()[0..n + 1]);
            }
        }

        Err(DecodingError::InvalidRemainingLength)
    }

    fn try_offset_variable_header(&self) -> Result<usize, DecodingError> {
        self.try_header().map(|header| header.len())
    }

    // Return the bytes forming the variable header.
    // The slice might be empty for packets without payload.
    fn try_variable_header(&self) -> Result<&[u8], DecodingError>;

    fn try_offset_payload(&self) -> Result<usize, DecodingError> {
        Ok(self.try_header().map(|header| header.len())?
            + self.try_variable_header().map(|header| header.len())?)
    }

    // Return the bytes forming the payload.
    // The slice might be empty for packets without payload.
    fn try_payload(&self) -> Result<&[u8], DecodingError> {
        let offset = self.try_offset_payload()?;
        let size = self.length() - offset;

        // TODO: Make lookup infallible.
        Ok(&self.as_bytes()[offset..offset + size])
    }
}
