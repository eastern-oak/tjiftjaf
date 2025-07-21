//! Providing [`Connect`]
use super::UnverifiedFrame;
use crate::{
    Frame, Packet, PacketType, ProtocolLevel, QoS,
    decode::{self, DecodingError},
    encode,
};
use bytes::{BufMut, Bytes, BytesMut};
use core::fmt;

/// [Connect](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718028) is the first packet a client sends, after a connection has been established.
///
/// # Example
///
/// Use a [`Builder`] to construct `Connect`.
/// ```
/// use tjiftjaf::packet_v2::connect::Connect;
///
/// let packet = Connect::builder()
///   .client_id("test")
///   .username("optimus")
///   .password("prime")
///   .build();
///
/// assert_eq!(packet.client_id(), "test");
/// assert_eq!(packet.username(), Some("optimus"));
/// assert_eq!(packet.password(), Some("prime".as_bytes()));
/// ```
///
/// Alternatively, try decoding [`Bytes`] as `Connect`.
/// ```
/// use tjiftjaf::packet_v2::connect::Connect;
/// use bytes::Bytes;
///
/// let frame = Bytes::copy_from_slice(&[16, 16, 0, 4, 77, 81, 84, 84, 4, 2, 1, 44, 0, 4, 116, 101, 115, 116]);
/// let packet = Connect::try_from(frame).unwrap();
/// assert_eq!(packet.client_id(), "test");
/// assert_eq!(packet.flags().clean_session(), true);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Connect {
    inner: UnverifiedConnect,
}

impl Connect {
    /// Creates a `Builder` to configure `Connect`.
    ///
    /// This is the same as [`Builder::new()`].
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Serialize `Connect`.
    pub fn into_bytes(self) -> Bytes {
        self.inner.inner
    }

    pub fn flags(&self) -> Flags {
        self.inner.connect_flags().unwrap()
    }

    pub fn client_id(&self) -> &str {
        self.inner.client_id().unwrap()
    }

    pub fn keep_alive(&self) -> u16 {
        self.inner.keep_alive().unwrap()
    }

    pub fn username(&self) -> Option<&str> {
        self.inner.username().unwrap()
    }

    pub fn password(&self) -> Option<&[u8]> {
        self.inner.password().unwrap()
    }

    pub fn will(&self) -> Option<Will> {
        self.inner.will().unwrap()
    }
}

impl Frame for Connect {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.inner.try_variable_header().unwrap()
    }
}

impl TryFrom<Bytes> for Connect {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        UnverifiedConnect { inner: value }.verify()
    }
}

impl From<Connect> for Bytes {
    fn from(value: Connect) -> Bytes {
        value.inner.inner
    }
}

impl From<Connect> for Packet {
    fn from(value: Connect) -> Packet {
        Packet::Connect(value)
    }
}

impl std::fmt::Debug for Connect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CONNECT")
            .field("length", &self.length())
            .field("client_id", &self.client_id())
            .field("keep_alive", &self.keep_alive())
            .field("username", &self.username())
            .field("will", &self.will())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct UnverifiedConnect {
    pub inner: Bytes,
}

impl UnverifiedConnect {
    fn keep_alive(&self) -> Result<u16, DecodingError> {
        let var_header = self.try_variable_header()?;
        decode::u16(&var_header[8..])
    }

    fn client_id(&self) -> Result<&str, DecodingError> {
        let (client_id, _) = decode::field::utf8(self.try_payload()?)?;
        Ok(client_id)
    }

    fn connect_flags(&self) -> Result<Flags, DecodingError> {
        let variable_header = self.try_variable_header()?;
        Ok(Flags(variable_header[7]))
    }

    fn will(&self) -> Result<Option<Will>, DecodingError> {
        let connect_flags = self.connect_flags()?;

        if !connect_flags.will_flag() {
            // TODO:
            // [MQTT-3.1.2-13] If the Will Flag is set to 0, then the Will QoS MUST be set to 0 (0x00)
            // [MQTT-3.1.2-15] If the Will Flag is set to 0, then the Will Retain Flag MUST be set to 0
            return Ok(None);
        }

        let payload = self.try_payload()?;

        let (will_topic, _) = decode::field::variable_length_n(&payload, 1)?;
        let will_topic = std::str::from_utf8(&will_topic)
            .map_err(|_| DecodingError::InvalidValue("Payload is not valid UTF-8".into()))?;
        let (will_message, _) = decode::field::variable_length_n(&payload, 2)?;

        Ok(Some(Will {
            topic: will_topic,
            message: will_message,
            retain: connect_flags.will_retain(),
            qos: connect_flags.will_qos(),
        }))
    }

    pub fn username(&self) -> Result<Option<&str>, DecodingError> {
        let connect_flags = self.connect_flags()?;
        if !connect_flags.username() {
            if connect_flags.password() {
                todo!("Illegal! Pas")
            }
            return Ok(None);
        };

        let payload = self.try_payload()?;
        let mut field_index = 1;

        // The will requires 2 fields: one for the topic and one for the message.
        if connect_flags.will_flag() {
            field_index = 3
        };

        let (username, _) = decode::field::variable_length_n(&payload, field_index)?;

        let username = std::str::from_utf8(&username)
            .map_err(|_| DecodingError::InvalidValue("Payload is not valid UTF-8".into()))?;
        Ok(Some(username))
    }

    pub fn password(&self) -> Result<Option<&[u8]>, DecodingError> {
        let connect_flags = self.connect_flags()?;
        if !connect_flags.password() {
            return Ok(None);
        };

        let payload = self.try_payload()?;
        let mut field_index = 2;

        // The will requires 2 fields: one for the topic and one for the message.
        if connect_flags.will_flag() {
            field_index = 4
        };

        let (password, _) = decode::field::variable_length_n(&payload, field_index)?;

        Ok(Some(password))
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != crate::PacketType::Connect {
            //  TODO return  correct packet type
            return Err(DecodingError::InvalidPacketType(5));
        }

        let packet_length = decode::packet_length(&header[1..header.len()])? as usize;
        if packet_length != self.length() {
            // TODO: Return  correct error
            return Err(DecodingError::TooManyBytes);
        }

        Ok(())
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        let header = self.try_variable_header()?;
        let (protocol_name, offset) = decode::field::utf8(header)?;
        assert_eq!(protocol_name, "MQTT");

        let protocol_level = header[offset];
        assert_eq!(protocol_level, ProtocolLevel::_3_1_1 as u8);

        let connect_flags = header[offset + 1];
        // Bit 0 must be 0, all other bits can be either 0 or 1.
        assert!(connect_flags & 1 == 0);

        Ok(())
    }

    fn verify_payload(&self) -> Result<(), DecodingError> {
        let payload = self.try_payload()?;
        // [MQTT-3.1.3-3] The Client Identifier (ClientId) MUST be present and MUST be the first field in the CONNECT packet payload.
        // [[MQTT-3.1.3-4] The ClientId MUST be a UTF-8 encoded string as defined in Section 1.5.3.
        let (client_id, _) = decode::field::utf8(payload)?;

        let connect_flags = self.connect_flags()?;

        // [MQTT-3.1.3-7] If the Client supplies a zero-byte ClientId, the Client MUST also set CleanSession to 1 .
        if client_id.is_empty() && !connect_flags.clean_session() {
            // Raise DecodingError;
            todo!()
        }

        // Try parsing fields related to will, username and password.
        self.will()?;
        self.username()?;
        self.password()?;

        Ok(())
    }

    fn verify(self) -> Result<Connect, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;
        self.verify_payload()?;

        Ok(Connect { inner: self })
    }
}

impl From<Bytes> for UnverifiedConnect {
    fn from(value: Bytes) -> Self {
        Self { inner: value }
    }
}

impl UnverifiedFrame for UnverifiedConnect {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        // The variable header of a CONNECT packet has a fixed size of 10 bytes.
        let offset = self.try_offset_variable_header()?;
        Ok(&self.as_bytes()[offset..offset + 10])
    }
}

#[derive(Clone, Default)]
pub struct Flags(pub(crate) u8);

impl Flags {
    pub fn username(&self) -> bool {
        self.0 & 128 == 128
    }

    pub fn set_username(&mut self) {
        self.0 |= 128
    }

    pub fn password(&self) -> bool {
        self.0 & 64 == 64
    }

    pub fn set_password(&mut self) {
        self.0 |= 64
    }

    pub fn will_retain(&self) -> bool {
        self.0 & 32 == 32
    }

    pub fn will_qos(&self) -> QoS {
        // TODO

        QoS::AtMostOnceDelivery
    }

    pub fn will_flag(&self) -> bool {
        self.0 & 4 == 4
    }

    pub fn clean_session(&self) -> bool {
        self.0 & 2 == 2
    }

    fn set_clean_session(&mut self) {
        self.0 |= 2;
    }
}

impl std::fmt::Debug for Flags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flags")
            .field("username", &self.username())
            .field("password", &self.password())
            .field("will_retain", &self.will_retain())
            .field("will_qos", &self.will_qos())
            .field("will_flag", &self.will_flag())
            .field("clean_session", &self.clean_session())
            .finish()
    }
}

#[derive(Debug)]
pub struct Will<'a> {
    pub topic: &'a str,
    // TODO: change to bytes
    pub message: &'a [u8],

    pub retain: bool,
    pub qos: QoS,
}

/// Helper type to construct a [`Connect`].
#[derive(Clone, Debug, Default)]
pub struct Builder {
    client_id: String,
    keep_alive: u16,

    username: Option<String>,
    password: Option<Vec<u8>>,
    flags: Flags,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            client_id: String::new(),
            keep_alive: 0,

            username: None,
            password: None,
            flags: Flags::default(),
        }
    }

    /// Configure the client id
    pub fn client_id(mut self, client_id: impl ToString) -> Self {
        self.client_id = client_id.to_string();
        self
    }

    /// Configure the keep alive interval.
    pub fn keep_alive(mut self, interval: u16) -> Self {
        self.keep_alive = interval;
        self
    }

    /// Configure the username.
    pub fn username(mut self, username: impl ToString) -> Self {
        self.username = Some(username.to_string());
        self.flags.set_username();
        self
    }

    /// Configure the password.
    pub fn password(mut self, password: impl Into<Vec<u8>>) -> Self {
        self.password = Some(password.into());
        self.flags.set_password();
        self
    }

    /// Try building a `Connect`.
    pub fn build(mut self) -> Connect {
        let mut fixed_header = BytesMut::with_capacity(2);
        fixed_header.put_u8((PacketType::Connect as u8) << 4);

        let mut variable_header = BytesMut::with_capacity(10);

        let protocol_name = encode::utf8("MQTT".into());
        variable_header.put(protocol_name);
        // Version of the protocol.
        variable_header.put_u8(ProtocolLevel::_3_1_1 as u8);

        // [MQTT-3.1.3-7] If the Client supplies a zero-byte ClientId, the Client MUST also set CleanSession to 1.
        if self.client_id.is_empty() {
            self.flags.set_clean_session();
        }

        // Connection flags
        variable_header.put_u8(self.flags.0);

        // Keep Alive
        variable_header.put_u16(self.keep_alive);

        let mut payload: BytesMut = encode::utf8(self.client_id).into();
        // TODO: Optionally add to payload
        // * will topic
        // * will message
        if self.flags.username() {
            let username = self.username.unwrap();
            payload.put_slice(&encode::utf8(username));

            if let Some(password) = self.password {
                payload.put_slice(&encode::bytes(&password));
            }
        }

        fixed_header.put_u8((variable_header.len() + payload.len()) as u8);

        fixed_header.put(variable_header);
        fixed_header.put(payload);

        UnverifiedConnect {
            inner: fixed_header.freeze(),
        }
        .verify()
        .unwrap()
    }

    pub fn build_packet(self) -> Packet {
        Packet::Connect(self.build())
    }
}
#[cfg(test)]
mod test {
    use crate::{Connect, Packet, connect, packet::Frame};
    use bytes::Bytes;

    #[test]
    fn test_connect() {
        let Packet::Connect(packet) = connect("test".to_string(), 300) else {
            panic!()
        };

        let bytes = Bytes::copy_from_slice(packet.as_bytes());
        let connect = Connect::try_from(bytes).unwrap();
        assert!(connect.will().is_none());

        let packet = Connect::builder().username("admin".to_string()).build();
        let bytes = Bytes::copy_from_slice(packet.as_bytes());

        let connect = Connect::try_from(bytes).unwrap();
        assert_eq!(connect.username(), Some("admin"));
        assert_eq!(connect.password(), None);
    }
}
