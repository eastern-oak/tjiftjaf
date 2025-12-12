//! Providing [`ConnAck`], a response from server to a `Connect`
use crate::{decode::DecodingError, Frame, Packet};
use bytes::Bytes;

/// [Connack](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718033)
#[derive(Clone, PartialEq, Eq)]
pub struct ConnAck {
    inner: [u8; 4],
}

impl ConnAck {
    /// Create a `ConnAckBuilder` to configure a `ConnAck`.
    pub fn builder() -> ConnAckBuilder {
        ConnAckBuilder::new()
    }

    /// Indicate if the server has stored state for this client.
    ///
    /// This might be `true` when a client reconnects and the server
    /// has collected some packets for the client.
    pub fn session_present(&self) -> bool {
        self.inner[2] & 1 == 1
    }

    /// Indication if connection was successful. If not, the `ReturnCode`
    /// explains the failure.
    pub fn return_code(&self) -> ReturnCode {
        ReturnCode::try_from(&self.inner[3]).unwrap()
    }
}

impl Frame for ConnAck {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        // This packet has a fixed length of 4 bytes.
        &self.as_bytes()[2..]
    }
}

impl From<ConnAck> for Bytes {
    fn from(value: ConnAck) -> Self {
        Bytes::copy_from_slice(&value.inner)
    }
}

impl From<ConnAck> for Packet {
    fn from(value: ConnAck) -> Self {
        Packet::ConnAck(value)
    }
}

impl TryFrom<Bytes> for ConnAck {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        match value.len() {
            0..=3 => {
                return Err(DecodingError::NotEnoughBytes {
                    minimum: 4,
                    actual: value.len(),
                });
            }
            4 => {}
            5.. => return Err(DecodingError::TooManyBytes {}),
        };

        if value[0] != 32 {
            return Err(DecodingError::InvalidPacketType(value[0]));
        };

        if value[1] != 2 {
            return Err(DecodingError::InvalidRemainingLength);
        }

        // Only the first bit of this field can be set. All other 7 bits
        // are not used.
        if value[2] > 1 {
            return Err(DecodingError::Other);
        }

        let return_code = ReturnCode::try_from(&value[3])?;

        //  [MQTT-3.2.2-4] If a server sends a CONNACK packet containing a non-zero return code it MUST set Session Present to 0 .
        if return_code != ReturnCode::ConnectionAccepted && value[2] != 0 {
            return Err(DecodingError::Other);
        }

        Ok(Self {
            // Unwrap is safe since we checked for size above.
            inner: value.as_ref().try_into().unwrap(),
        })
    }
}

impl std::fmt::Debug for ConnAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CONNACK")
            .field("length", &self.length())
            .field("session_present", &self.session_present())
            .field("return_code", &self.return_code())
            .finish()
    }
}

/// A status code indicating if client connected successfully
/// to the server. If not, the return code provides a hint
/// why the connection failed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReturnCode {
    ConnectionAccepted = 0x0,

    /// The Server does not support the level of the MQTT protocol requested by the Client.
    ConnectionRefusedUnacceptableProtocolVersion = 0x1,

    /// The Client identifier is correct UTF-8 but not allowed by the Server
    ConnectionRefusedIdentifierRejected = 0x2,

    /// The Network Connection has been made but the MQTT service is unavailable.
    ConnectionRefusedServerUnavailable = 0x3,

    /// The data in the user name or password is malformed
    ConnectionRefusedBadUsernameOrPassword = 0x4,

    /// The Client is not authorized to connect
    ConnectionRefusedNotAuthorized = 0x5,
}

impl TryFrom<&u8> for ReturnCode {
    type Error = DecodingError;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        let value = match value {
            0x0 => Self::ConnectionAccepted,
            0x1 => Self::ConnectionRefusedUnacceptableProtocolVersion,
            0x2 => Self::ConnectionRefusedIdentifierRejected,
            0x3 => Self::ConnectionRefusedServerUnavailable,
            0x4 => Self::ConnectionRefusedBadUsernameOrPassword,
            0x5 => Self::ConnectionRefusedNotAuthorized,
            _ => {
                return Err(DecodingError::InvalidValue(format!(
                    "{value} is not a valid value for ReturnCode",
                )));
            }
        };

        Ok(value)
    }
}

impl From<ReturnCode> for u8 {
    fn from(value: ReturnCode) -> Self {
        match value {
            ReturnCode::ConnectionAccepted => 0x0,
            ReturnCode::ConnectionRefusedUnacceptableProtocolVersion => 0x01,
            ReturnCode::ConnectionRefusedIdentifierRejected => 0x02,
            ReturnCode::ConnectionRefusedServerUnavailable => 0x03,
            ReturnCode::ConnectionRefusedBadUsernameOrPassword => 0x04,
            ReturnCode::ConnectionRefusedNotAuthorized => 0x5,
        }
    }
}

/// A helper type to create a `ConnAck`.
pub struct ConnAckBuilder {
    return_code: ReturnCode,
    session_present: bool,
}

impl ConnAckBuilder {
    pub fn new() -> Self {
        Self {
            session_present: false,
            return_code: ReturnCode::ConnectionAccepted,
        }
    }

    /// Set the [session present](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc385349255) bit.
    ///
    /// This bit indicates that the server has stored state
    /// for the client that just connected.
    pub fn session_present(mut self) -> Self {
        self.session_present = true;
        self
    }

    /// Configure the `ReturnCode`.
    pub fn return_code(mut self, return_code: ReturnCode) -> Self {
        self.return_code = return_code;
        self
    }

    /// Returns a `ConnAck` using the `ConnAckBuilder` configuration.
    pub fn build(self) -> ConnAck {
        ConnAck {
            inner: [
                2 << 4,
                2,
                self.session_present as u8,
                self.return_code.into(),
            ],
        }
    }
}

impl Default for ConnAckBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use crate::{packet::connack::ReturnCode, ConnAck};
    use bytes::Bytes;

    #[test]
    fn test_building_connack() {
        let connack = ConnAck::builder().build();
        assert!(!connack.session_present());
        assert_eq!(connack.return_code(), ReturnCode::ConnectionAccepted);

        let connack = ConnAck::builder()
            .session_present()
            .return_code(ReturnCode::ConnectionRefusedNotAuthorized)
            .build();
        assert!(connack.session_present());
        assert_eq!(
            connack.return_code(),
            ReturnCode::ConnectionRefusedNotAuthorized
        );
    }

    #[test]
    fn test_decode_connack() {
        // This input is too short.
        let input = Bytes::copy_from_slice(&[32, 2, 0]);
        assert!(ConnAck::try_from(input).is_err());

        // This input is too long.
        let input = Bytes::copy_from_slice(&[32, 2, 0, 0, 0]);
        assert!(ConnAck::try_from(input).is_err());
    }
}
