//! Providing the [`SubAck`] frame, used by a broker to confirm a [`crate::Subscribe`].
use crate::{
    decode::{self, DecodingError},
    encode,
    packet::UnverifiedFrame,
    Frame, Packet, PacketType, QoS,
};

/// [SubAck](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718068) is emitted by a broker to confirm a [`crate::Subscribe`] request.
///
/// # Example
///
/// Use a [`Builder`] to construct `SubAck`.
///
/// ```
/// use tjiftjaf::{SubAck, QoS, packet::suback::ReturnCode};
///
/// let frame = SubAck::builder(1522, QoS::AtMostOnceDelivery)
///         .add_return_code(ReturnCode::Failure)
///         .build();
///
/// assert_eq!(frame.packet_identifier(), 1522);
/// assert_eq!(frame.return_codes(), vec![ReturnCode::QoS(QoS::AtMostOnceDelivery), ReturnCode::Failure]);
/// ```
///
/// Alternatively, try decoding some bytes as `SubAck`.
///
/// ```
/// use tjiftjaf::{SubAck, QoS, packet::suback::ReturnCode};
///
/// let frame = vec![144, 3, 55, 219, 0];
/// let packet = SubAck::try_from(frame).unwrap();
/// assert_eq!(packet.packet_identifier(), 14299);
/// assert_eq!(packet.return_codes(), vec![ReturnCode::QoS(QoS::AtMostOnceDelivery)]);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct SubAck {
    inner: UnverifiedSubAck,
}

impl SubAck {
    /// Serialize `SubAck`.
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.inner
    }

    /// Creates a [`Builder`] to configure `SubAck`.
    pub fn builder(packet_identifier: u16, return_code: impl Into<ReturnCode>) -> Builder {
        Builder::new(packet_identifier, return_code)
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.inner.try_packet_identifier().unwrap()
    }

    /// Returns an iterator over the return codes.
    ///
    /// # Example
    ///
    /// ```
    /// use tjiftjaf::{SubAck, QoS, packet::suback::ReturnCode};
    ///
    /// let frame = SubAck::builder(1522, QoS::AtMostOnceDelivery)
    ///         .add_return_code(ReturnCode::Failure)
    ///         .build();
    ///
    /// assert_eq!(frame.packet_identifier(), 1522);
    /// assert_eq!(frame.return_codes(), vec![ReturnCode::QoS(QoS::AtMostOnceDelivery), ReturnCode::Failure]);
    /// ```
    pub fn return_codes(&self) -> Vec<ReturnCode> {
        self.inner.try_return_codes().unwrap()
    }
}

impl Frame for SubAck {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        &self.as_bytes()[offset..offset + 2]
    }
}

impl TryFrom<Vec<u8>> for SubAck {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        UnverifiedSubAck { inner: value }.verify()
    }
}

impl From<SubAck> for Vec<u8> {
    fn from(value: SubAck) -> Vec<u8> {
        value.inner.inner
    }
}

impl From<SubAck> for Packet {
    fn from(value: SubAck) -> Packet {
        Packet::SubAck(value)
    }
}

impl std::fmt::Debug for SubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SUBACK")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .field("return_codes", &self.return_codes())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct UnverifiedSubAck {
    pub inner: Vec<u8>,
}

impl UnverifiedSubAck {
    fn try_packet_identifier(&self) -> Result<u16, DecodingError> {
        let header = self.try_variable_header()?;
        decode::u16(header)
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != crate::PacketType::SubAck {
            //  TODO return  correct packet type
            return Err(DecodingError::InvalidPacketType(packet_type as u8));
        }

        // The lowest 4 bits of the header include flags.
        // For SUBACK, none of these flags is set.
        if header[0] & 0b1111 != 0 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }

        // TODO: limit payload length to 255.
        let packet_length = decode::packet_length(&header[1..header.len()])? as usize;
        if packet_length != self.length() {
            // TODO: Return  correct error
            return Err(DecodingError::TooManyBytes);
        }

        Ok(())
    }

    fn try_return_codes(&self) -> Result<Vec<ReturnCode>, DecodingError> {
        self.try_payload()?
            .iter()
            .map(ReturnCode::try_from)
            .collect::<Result<Vec<ReturnCode>, InvalidReturnCode>>()
            .map_err(|error| {
                DecodingError::InvalidValue(format!("{} is not a valid ReturnCode", error.0))
            })
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        self.try_variable_header()?;
        Ok(())
    }

    fn verify_payload(&self) -> Result<(), DecodingError> {
        self.try_return_codes()?;

        // TODO: check that payload is not empty
        Ok(())
    }

    fn verify(self) -> Result<SubAck, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;
        self.verify_payload()?;

        Ok(SubAck { inner: self })
    }
}

impl UnverifiedFrame for UnverifiedSubAck {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        // The variable header of a SUBACK packet has a fixed size of 2 bytes.
        let offset = self.try_offset_variable_header()?;
        Ok(&self.as_bytes()[offset..offset + 2])
    }
}

pub struct Builder {
    packet_identifier: u16,
    return_codes: Vec<ReturnCode>,
}

impl Builder {
    pub fn new(packet_identifier: u16, return_code: impl Into<ReturnCode>) -> Self {
        Self {
            packet_identifier,
            return_codes: vec![return_code.into()],
        }
    }

    pub fn add_return_code(mut self, return_code: impl Into<ReturnCode>) -> Self {
        self.return_codes.push(return_code.into());
        self
    }

    pub fn build(self) -> SubAck {
        let mut variable_header = self.packet_identifier.to_be_bytes().to_vec();

        let mut payload: Vec<u8> = Vec::with_capacity(self.return_codes.len());
        for code in self.return_codes {
            payload.push(code.into())
        }

        let mut packet: Vec<u8> = Vec::new();
        let packet_type: u8 = PacketType::SubAck.into();
        packet.push(packet_type << 4);

        let remaining_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.append(&mut remaining_length.to_vec());
        packet.append(&mut variable_header);
        packet.append(&mut payload);

        UnverifiedSubAck { inner: packet }.verify().unwrap()
    }

    pub fn build_packet(self) -> Packet {
        Packet::SubAck(self.build())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnCode {
    QoS(QoS),
    Failure,
}

impl From<QoS> for ReturnCode {
    fn from(value: QoS) -> Self {
        ReturnCode::QoS(value)
    }
}

impl From<ReturnCode> for u8 {
    fn from(value: ReturnCode) -> Self {
        match value {
            ReturnCode::QoS(qos) => qos as u8,
            ReturnCode::Failure => 0x80,
        }
    }
}

impl TryFrom<&u8> for ReturnCode {
    type Error = InvalidReturnCode;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        let variant = match value {
            0x0 => Self::QoS(QoS::AtMostOnceDelivery),
            0x1 => Self::QoS(QoS::AtLeastOnceDelivery),
            0x2 => Self::QoS(QoS::ExactlyOnceDelivery),
            0x80 => Self::Failure,
            _ => return Err(InvalidReturnCode(*value)),
        };

        Ok(variant)
    }
}

impl TryFrom<u8> for ReturnCode {
    type Error = InvalidReturnCode;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        ReturnCode::try_from(&value)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct InvalidReturnCode(u8);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_suback() {
        let frame = SubAck::builder(14299, QoS::AtMostOnceDelivery).build();
        dbg!(frame.as_bytes());
        let _: SubAck = frame.into_bytes().try_into().unwrap();

        let frame = SubAck::builder(1522, QoS::AtMostOnceDelivery)
            .add_return_code(QoS::AtLeastOnceDelivery)
            .build();
        let _: SubAck = frame.into_bytes().try_into().unwrap();
    }
}
