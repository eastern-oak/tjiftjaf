//! Providing [`Connect`], the first message a client sends to the server.
use super::UnverifiedFrame;
use crate::{
    decode::{self, DecodingError},
    encode, Frame, Packet, PacketType, ProtocolLevel, QoS,
};
use bytes::{BufMut, Bytes, BytesMut};
use core::fmt;
use std::marker::PhantomData;

/// [Connect](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718028) is the first packet a client sends, after a connection has been established.
///
/// # Example
///
/// Use a [`Builder`] to construct `Connect`.
/// ```
/// use tjiftjaf::Connect;
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
/// use tjiftjaf::Connect;
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

    pub fn into_bytes(self) -> Bytes {
        self.inner.inner
    }

    /// Retrieve the connection flags.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().username("optimus").build();
    /// assert_eq!(packet.flags().username(), true);
    /// assert_eq!(packet.flags().password(), false);
    /// ```
    pub fn flags(&self) -> Flags {
        self.inner.connect_flags().unwrap()
    }

    /// Retrieve client id.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.client_id(), "");
    ///
    /// let packet = Connect::builder().client_id("host-23").build();
    /// assert_eq!(packet.client_id(), "host-23");
    /// ```
    pub fn client_id(&self) -> &str {
        self.inner.client_id().unwrap()
    }

    /// Retrieve the keep alive interval in seconds.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.keep_alive(), 0);
    ///
    /// let packet = Connect::builder().keep_alive(60).build();
    /// assert_eq!(packet.keep_alive(), 60);
    /// ```
    pub fn keep_alive(&self) -> u16 {
        self.inner.keep_alive().unwrap()
    }

    /// Retrieve the username, if configured.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.username(), None);
    ///
    /// let packet = Connect::builder().username("optimus").build();
    /// assert_eq!(packet.username(), Some("optimus"));
    /// ```
    pub fn username(&self) -> Option<&str> {
        self.inner.username().unwrap()
    }

    /// Retrieve the password, if configured.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().username("optimus").build();
    /// assert_eq!(packet.password(), None);
    ///
    /// let packet = Connect::builder()
    ///     .username("optimus")
    ///     .password("prime")
    ///     .build();
    /// assert_eq!(packet.password(), Some("prime".as_bytes()));
    /// ```
    pub fn password(&self) -> Option<&[u8]> {
        self.inner.password().unwrap()
    }

    /// Retrieve the `Will`, if configured.
    ///
    /// ```
    /// use tjiftjaf::{QoS, Connect};
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.will(), None);
    ///
    /// let packet = Connect::builder()
    ///     .will("topic", "optimus died")
    ///     .retain_will()
    ///     .build();
    ///
    /// let will = packet.will().unwrap();
    /// assert_eq!(will.topic, "topic");
    /// assert_eq!(will.message, b"optimus died");
    /// assert_eq!(will.qos, QoS::AtMostOnceDelivery);
    /// assert_eq!(will.retain, true);
    /// ```
    pub fn will(&self) -> Option<Will<'_>> {
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

    fn will(&self) -> Result<Option<Will<'_>>, DecodingError> {
        let connect_flags = self.connect_flags()?;

        if !connect_flags.will_flag() {
            // TODO:
            // [MQTT-3.1.2-13] If the Will Flag is set to 0, then the Will QoS MUST be set to 0 (0x00)
            // [MQTT-3.1.2-15] If the Will Flag is set to 0, then the Will Retain Flag MUST be set to 0
            return Ok(None);
        }

        let payload = self.try_payload()?;

        let (will_topic, _) = decode::field::variable_length_n(payload, 1)?;
        let will_topic = std::str::from_utf8(will_topic)
            .map_err(|_| DecodingError::InvalidValue("Payload is not valid UTF-8".into()))?;
        let (will_message, _) = decode::field::variable_length_n(payload, 2)?;

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

        let (username, _) = decode::field::variable_length_n(payload, field_index)?;

        let username = std::str::from_utf8(username)
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

        let (password, _) = decode::field::variable_length_n(payload, field_index)?;

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
    /// Whether the username is set.
    pub fn username(&self) -> bool {
        self.0 & 128 == 128
    }

    pub(crate) fn set_username(&mut self) {
        self.0 |= 128
    }

    /// Whether the password is set.
    pub fn password(&self) -> bool {
        self.0 & 64 == 64
    }

    pub(crate) fn set_password(&mut self) {
        self.0 |= 64
    }

    /// Whether the will retain bit is set.
    pub fn will_retain(&self) -> bool {
        self.0 & 32 == 32
    }

    pub(crate) fn set_will_retain(&mut self) {
        self.0 |= 32
    }

    /// The QoS of the will.
    pub fn will_qos(&self) -> QoS {
        QoS::try_from((self.0 & 24) >> 3).unwrap()
    }

    pub(crate) fn set_will_qos(&mut self, qos: QoS) {
        self.0 |= (qos as u8) << 3;
    }

    /// Whether a will is configured.
    pub fn will_flag(&self) -> bool {
        self.0 & 4 == 4
    }

    pub(crate) fn set_will_flag(&mut self) {
        self.0 |= 4
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Will<'a> {
    pub topic: &'a str,
    // TODO: change to bytes
    pub message: &'a [u8],

    pub retain: bool,
    pub qos: QoS,
}

/// A marker to indicate that [`Builder`] does not include credentials.
#[derive(Copy, Clone, Debug)]
pub struct WithoutAuth;

/// A marker to indicate that [`Builder`] includes credentials.
#[derive(Copy, Clone, Debug)]
pub struct WithAuth;

/// A marker to indicate that [`Builder`] does not include a configuration for a will.
#[derive(Copy, Clone, Debug)]
pub struct WithoutWill;

/// A marker to indicate that [`Builder`] includes a configuration for a will.
#[derive(Copy, Clone, Debug)]
pub struct WithWill;

/// Helper type to construct a [`Connect`].
///
/// ```
/// use tjiftjaf::Connect;
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
#[derive(Clone, Default)]
pub struct Builder<A = WithoutAuth, W = WithoutWill> {
    client_id: String,
    keep_alive: u16,

    will_topic: Option<String>,
    will_message: Option<Vec<u8>>,
    username: Option<String>,
    password: Option<Vec<u8>>,
    flags: Flags,

    _auth: PhantomData<A>,
    _will: PhantomData<W>,
}

impl Builder<WithoutAuth, WithoutWill> {
    pub fn new() -> Self {
        Builder {
            client_id: String::new(),
            keep_alive: 0,

            username: None,
            password: None,
            will_topic: None,
            will_message: None,
            flags: Flags::default(),
            _auth: PhantomData,
            _will: PhantomData,
        }
    }
}

impl<A, W> Builder<A, W> {
    /// Configure the client id.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.client_id(), "");
    ///
    /// let packet = Connect::builder().client_id("host-23").build();
    /// assert_eq!(packet.client_id(), "host-23");
    /// ```
    pub fn client_id(mut self, client_id: impl ToString) -> Self {
        self.client_id = client_id.to_string();
        self
    }

    /// Configure the keep alive interval.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.keep_alive(), 0);
    ///
    /// let packet = Connect::builder().keep_alive(60).build();
    /// assert_eq!(packet.keep_alive(), 60);
    /// ```
    pub fn keep_alive(mut self, interval: u16) -> Self {
        self.keep_alive = interval;
        self
    }

    /// Configure the username.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.username(), None);
    ///
    /// let packet = Connect::builder().username("optimus").build();
    /// assert_eq!(packet.username(), Some("optimus"));
    /// ```
    pub fn username(mut self, username: impl ToString) -> Builder<WithAuth, W> {
        let auth: PhantomData<WithAuth> = PhantomData;
        self.flags.set_username();
        Builder {
            client_id: self.client_id,
            keep_alive: self.keep_alive,
            will_topic: self.will_topic,
            will_message: self.will_message,
            username: Some(username.to_string()),
            password: self.password,
            flags: self.flags,
            _auth: auth,
            _will: self._will,
        }
    }

    /// Configure the will.
    ///
    /// ```
    /// use tjiftjaf::{QoS, Connect};
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.will(), None);
    ///
    /// let packet = Connect::builder()
    ///     .will("topic", "optimus died")
    ///     .retain_will()
    ///     .build();
    ///
    /// let will = packet.will().unwrap();
    /// assert_eq!(will.topic, "topic");
    /// assert_eq!(will.message, b"optimus died");
    /// assert_eq!(will.qos, QoS::AtMostOnceDelivery);
    /// assert_eq!(will.retain, true);
    /// ```
    pub fn will(
        mut self,
        topic: impl Into<String>,
        message: impl Into<Vec<u8>>,
    ) -> Builder<A, WithWill> {
        let will: PhantomData<WithWill> = PhantomData;
        self.flags.set_will_flag();
        Builder {
            client_id: self.client_id,
            keep_alive: self.keep_alive,
            will_topic: Some(topic.into()),
            will_message: Some(message.into()),
            username: self.username,
            password: self.password,
            flags: self.flags,
            _auth: self._auth,
            _will: will,
        }
    }

    /// Set the clean session flag.
    ///
    /// ```
    /// use tjiftjaf::{QoS, Connect};
    ///
    /// let packet = Connect::builder().build();
    ///
    /// // Without client id, the clean session flag is always true.
    /// assert_eq!(packet.client_id(), "");
    /// assert_eq!(packet.flags().clean_session(), true);
    ///
    /// let packet = Connect::builder()
    ///     .client_id("client-1")
    ///     .build();
    /// assert_eq!(packet.flags().clean_session(), false);
    ///
    /// let packet = Connect::builder()
    ///     .client_id("client-1")
    ///     .clean_session()
    ///     .build();
    ///
    /// assert_eq!(packet.flags().clean_session(), true);
    /// ```
    pub fn clean_session(mut self) -> Self {
        self.flags.set_clean_session();
        self
    }

    /// Build a `Connect`.
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
        if let Some(will_topic) = self.will_topic {
            payload.put_slice(&encode::utf8(will_topic));
        }

        if let Some(will_message) = self.will_message {
            payload.put_slice(&encode::bytes(&will_message));
        }

        if let Some(username) = self.username {
            payload.put_slice(&encode::utf8(username));

            if let Some(password) = self.password {
                payload.put_slice(&encode::bytes(&password));
            }
        }

        fixed_header.put(encode::remaining_length(
            variable_header.len() + payload.len(),
        ));

        fixed_header.put(variable_header);
        fixed_header.put(payload);

        UnverifiedConnect {
            inner: fixed_header.freeze()
        }
        .verify()
        .unwrap_or_else(|e| panic!("`Builder` failed to build `Connect`. This is a bug. Please report it to https://github.com/eastern-oak/tjiftjaf/issues. The error is '{e}'."))
    }

    pub fn build_packet(self) -> Packet {
        Packet::Connect(self.build())
    }
}

impl<WithAuth, W> Builder<WithAuth, W> {
    /// Configure the password.
    ///
    /// ```
    /// use tjiftjaf::Connect;
    ///
    /// let packet = Connect::builder().username("optimus").build();
    /// assert_eq!(packet.password(), None);
    ///
    /// let packet = Connect::builder()
    ///     .username("optimus")
    ///     .password("prime")
    ///     .build();
    /// assert_eq!(packet.password(), Some("prime".as_bytes()));
    /// ```
    pub fn password(mut self, password: impl Into<Vec<u8>>) -> Self {
        self.flags.set_password();
        self.password = Some(password.into());
        self
    }
}

impl<A, WithWill> Builder<A, WithWill> {
    /// Configure the `QoS` of the will message.
    /// ```
    /// use tjiftjaf::{QoS, Connect};
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.will(), None);
    ///
    /// let packet = Connect::builder()
    ///     .will("topic", "optimus died")
    ///     .will_qos(QoS::ExactlyOnceDelivery)
    ///     .build();
    ///
    /// let will = packet.will().unwrap();
    /// assert_eq!(will.topic, "topic");
    /// assert_eq!(will.message, b"optimus died");
    /// assert_eq!(will.qos, QoS::ExactlyOnceDelivery);
    /// assert_eq!(will.retain, false);
    /// ```
    pub fn will_qos(mut self, qos: QoS) -> Self {
        self.flags.set_will_qos(qos);
        self
    }

    /// Retain the will topic and will message on the server.
    /// That allows the message to be delivered to future subscribers to the will topic.
    ///
    /// ```
    /// use tjiftjaf::{QoS, Connect};
    ///
    /// let packet = Connect::builder().build();
    /// assert_eq!(packet.will(), None);
    ///
    /// let packet = Connect::builder()
    ///     .will("topic", "optimus died")
    ///     .retain_will()
    ///     .build();
    ///
    /// let will = packet.will().unwrap();
    /// assert_eq!(will.topic, "topic");
    /// assert_eq!(will.message, b"optimus died");
    /// assert_eq!(will.qos, QoS::AtMostOnceDelivery);
    /// assert_eq!(will.retain, true);
    /// ```
    pub fn retain_will(mut self) -> Self {
        self.flags.set_will_retain();
        self
    }
}

impl<A, W> std::fmt::Debug for Builder<A, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("client_id", &self.client_id)
            .field("keep_alive", &self.keep_alive)
            .field("will_topic", &self.will_topic)
            .field("will_message", &self.will_message)
            .field("username", &self.username)
            .field("password", &self.password)
            .field("flags", &self.flags)
            .finish()
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for Connect {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let mut builder = Connect::builder();
        if bool::arbitrary(u)? {
            builder = builder.clean_session();
        };

        if bool::arbitrary(u)? {
            return Ok(builder.build());
        }

        let mut builder = builder.username(String::arbitrary(u).unwrap());
        if bool::arbitrary(u)? {
            builder = builder.password(Vec::<u8>::arbitrary(u).unwrap());
        }

        if bool::arbitrary(u)? {
            return Ok(builder.build());
        }

        let mut builder = builder.will(
            String::arbitrary(u).unwrap(),
            Vec::<u8>::arbitrary(u).unwrap(),
        );
        if bool::arbitrary(u)? {
            builder = builder.retain_will();
        }

        let choices = [QoS::AtMostOnceDelivery, QoS::AtLeastOnceDelivery];
        builder = builder.will_qos(*u.choose(&choices)?);

        Ok(builder.build())
    }
}

#[cfg(test)]
mod test {
    use crate::{packet::Frame, Connect};
    use bytes::Bytes;

    #[test]
    fn test_connect() {
        let packet = Connect::builder().build();

        let bytes = Bytes::copy_from_slice(packet.as_bytes());
        let connect = Connect::try_from(bytes).unwrap();
        assert!(connect.will().is_none());

        let packet = Connect::builder().username("admin").build();
        let bytes = Bytes::copy_from_slice(packet.as_bytes());

        let connect = Connect::try_from(bytes).unwrap();
        assert_eq!(connect.username(), Some("admin"));
        assert_eq!(connect.password(), None);
    }

    /// #61 tracks a bug where `connect::Builder.build()` encoded the length
    /// of the packet in a single byte. This is wrong. The encoded length can take
    /// up to 4 bytes for larger packets.
    ///
    /// This test creates a large packet, encodes it and then decodes it again.
    /// Before the fix, the last step would fail because the encoded length in the packet
    /// wouldn't match the actual length.
    #[test]
    fn test_gh_61_fix_for_building_long_connect_packet() {
        let packet = Connect::builder().will("topic", [0; 255]).build();
        assert!(Connect::try_from(Bytes::copy_from_slice(packet.as_bytes())).is_ok());
    }
}
