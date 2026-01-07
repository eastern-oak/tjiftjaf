//! Providing [`Publish`], used by both client and server to send a message on a topic.
use crate::{
    decode::{self, DecodingError},
    encode,
    packet::UnverifiedFrame,
    packet_identifier, ConnectionError, Frame, Packet, PacketType, QoS,
};

/// [Publish](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718037) is used by both clients and servers
/// to emit a message for a specific topic.
///
/// # Example
///
/// Use a [`Builder`] to construct `Publish`.
/// ```
/// use tjiftjaf::{QoS, Publish};
///
/// let packet = Publish::builder("test/topic", "Hello MQTT!")
///     .qos(QoS::AtMostOnceDelivery)
///     .retain(true)
///     .build();
///
/// assert_eq!(packet.topic(), "test/topic");
/// assert_eq!(packet.payload(), b"Hello MQTT!");
/// assert_eq!(packet.qos(), QoS::AtMostOnceDelivery);
/// ```
///
/// Alternatively, decode `Publish` from some bytes:
///
/// ```
/// use tjiftjaf::Publish;
/// use bytes::Bytes;
///
/// let frame = Bytes::copy_from_slice(&[49, 23, 0, 10, 116, 101, 115, 116, 47, 116, 111, 112, 105, 99, 72, 101, 108, 108, 111, 32, 77, 81, 84, 84, 33]);
/// let packet = Publish::try_from(frame).unwrap();
/// assert_eq!(packet.topic(), "test/topic");
/// assert_eq!(packet.payload(), b"Hello MQTT!");
/// assert_eq!(packet.packet_identifier(), None);
/// ```
///
#[derive(Clone, PartialEq, Eq)]
pub struct Publish {
    inner: UnverifiedPublish,
}

impl Publish {
    /// Creates a [`Builder`] to configure `Publish`.
    pub fn builder(topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> Builder {
        Builder::new(topic, payload)
    }

    /// Serialize `Publish`.
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.inner
    }

    /// Get the topic this message is published to.
    pub fn topic(&self) -> &str {
        self.inner.topic().unwrap()
    }

    /// Get the payload of this message.
    pub fn payload(&self) -> &[u8] {
        self.inner.payload().unwrap()
    }

    /// Get the QoS level of this message.
    pub fn qos(&self) -> QoS {
        self.inner.qos().unwrap()
    }

    /// Returns whether this message should be retained by the broker.
    pub fn retain(&self) -> bool {
        self.inner.retain().unwrap()
    }

    /// Returns whether this is a duplicate message.
    pub fn duplicate(&self) -> bool {
        self.inner.duplicate().unwrap()
    }

    /// Get the packet identifier if QoS > 0.
    pub fn packet_identifier(&self) -> Option<u16> {
        self.inner.packet_identifier().unwrap()
    }
}

impl Frame for Publish {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.inner.try_variable_header().unwrap()
    }
}

impl std::fmt::Debug for Publish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBLISH")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .field("topic", &self.topic())
            .field("payload_length", &self.payload().len())
            .field("qos", &self.qos())
            .field("retain", &self.retain())
            .field("duplicate", &self.duplicate())
            .finish()
    }
}
impl TryFrom<Vec<u8>> for Publish {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        UnverifiedPublish { inner: value }.verify()
    }
}

impl From<Publish> for Vec<u8> {
    fn from(value: Publish) -> Vec<u8> {
        value.inner.inner
    }
}

impl From<Publish> for Packet {
    fn from(value: Publish) -> Packet {
        Packet::Publish(value)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct UnverifiedPublish {
    pub inner: Vec<u8>,
}

impl UnverifiedPublish {
    fn topic(&self) -> Result<&str, DecodingError> {
        let var_header = self.try_variable_header()?;
        let (topic, _) = decode::field::utf8(var_header)?;
        Ok(topic)
    }

    fn payload(&self) -> Result<&[u8], DecodingError> {
        self.try_payload()
    }

    fn qos(&self) -> Result<QoS, DecodingError> {
        let header = self.try_header()?;
        let flags = header[0] >> 1 & 0b11;
        QoS::try_from(flags).map_err(|_| DecodingError::InvalidValue("Invalid QoS value".into()))
    }

    fn retain(&self) -> Result<bool, DecodingError> {
        let header = self.try_header()?;
        Ok(header[0] & 0b0001 == 0b0001)
    }

    fn duplicate(&self) -> Result<bool, DecodingError> {
        let header = self.try_header()?;
        Ok(header[0] & 0b1000 == 0b1000)
    }

    fn packet_identifier(&self) -> Result<Option<u16>, DecodingError> {
        let variable_header = self.try_variable_header()?;
        let (_, offset) = decode::field::utf8(variable_header)?;

        if self.qos()? == QoS::AtMostOnceDelivery {
            if variable_header.len() == offset {
                return Ok(None);
            }

            // A packet identifier is not  allowed with QoS 0
            return Err(DecodingError::Other);
        }

        let identifier = decode::u16(&variable_header[offset..])?;
        Ok(Some(identifier))
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != PacketType::Publish {
            return Err(DecodingError::InvalidPacketType(3));
        }

        let packet_length = decode::packet_length(&header[1..header.len()])? as usize;
        if packet_length != self.length() {
            return Err(DecodingError::TooManyBytes);
        }

        Ok(())
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        self.topic()?;
        self.packet_identifier()?;

        Ok(())
    }

    fn verify(self) -> Result<Publish, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;

        Ok(Publish { inner: self })
    }
}

impl UnverifiedFrame for UnverifiedPublish {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        let offset = self.try_offset_variable_header()?;
        let mut len = 0;

        // Calculate variable header length (topic + optional packet identifier)
        let (_, topic_len) = decode::field::utf8(&self.inner[offset..])?;
        len += topic_len;

        if self.qos()? != QoS::AtMostOnceDelivery {
            len += 2; // Packet identifier length
        }

        Ok(&self.as_bytes()[offset..offset + len])
    }
}

/// Helper type to construct a [`Publish`].
#[derive(Clone, Debug)]
pub struct Builder {
    topic: String,
    payload: Vec<u8>,
    qos: QoS,
    retain: bool,
    duplicate: bool,
    packet_identifier: Option<u16>,
}

impl Builder {
    pub fn new(topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        Builder {
            topic: topic.into(),
            payload: payload.into(),
            qos: QoS::AtMostOnceDelivery,
            retain: false,
            duplicate: false,
            packet_identifier: None,
        }
    }

    /// Set the QoS level.
    pub fn qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    /// Set whether the message should be retained.
    pub fn retain(mut self, retain: bool) -> Self {
        self.retain = retain;
        self
    }

    /// Set whether this is a duplicate message.
    pub fn duplicate(mut self, dup: bool) -> Self {
        self.duplicate = dup;
        self
    }

    /// Set the packet identifier (required for QoS > 0).
    pub fn packet_identifier(mut self, id: u16) -> Self {
        self.packet_identifier = Some(id);
        self
    }

    /// Build the `Publish` packet.
    pub fn build(mut self) -> Publish {
        // The 4 least significant bits configure
        // * Retain
        // * QoS
        // * Duplicate
        //
        //      3 | 2 1 |   0
        //   +----+-----+-------+
        //    DUP | QoS | RETAIN
        let mut flags = 0b0000;
        if self.retain {
            flags |= 0b0001;
        }
        flags |= (self.qos as u8) << 1;
        if self.duplicate {
            flags |= 0b1000;
        }

        let mut fixed_header = Vec::new();
        fixed_header.push((PacketType::Publish as u8) << 4 | flags);

        let mut variable_header = Vec::new();
        variable_header.append(&mut encode::utf8(self.topic).to_vec());

        // The Packet Identifier field is only present in PUBLISH Packets where the QoS level is 1 or 2. Section 2.3.1 provides more information about Packet Identifiers.
        if self.qos != QoS::AtMostOnceDelivery {
            variable_header.append(
                &mut self
                    .packet_identifier
                    .unwrap_or_else(packet_identifier)
                    .to_be_bytes()
                    .to_vec(),
            );
        }

        let mut payload = Vec::new();
        payload.append(&mut self.payload);

        let remaining_length = encode::remaining_length(variable_header.len() + payload.len());
        fixed_header.append(&mut remaining_length.to_vec());
        fixed_header.append(&mut variable_header);
        fixed_header.append(&mut payload);

        UnverifiedPublish {
            inner: fixed_header,
        }
        .verify()
        .unwrap()
    }

    /// Build a `Packet::Publish`.
    pub fn build_packet(self) -> Packet {
        Packet::Publish(self.build())
    }
}

#[cfg(feature = "async")]
impl crate::aio::Emit for Publish {
    /// Publish `payload` to the given `topic`.
    ///
    /// ```no_run
    /// use bytes::Bytes;
    /// # use async_net::TcpStream;
    /// # use futures_lite::FutureExt;
    /// # use tjiftjaf::{publish, Connect, QoS, aio::{Client, Emit}, packet_identifier};
    /// # smol::block_on(async {
    /// # let stream = TcpStream::connect("localhost:1883").await.unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, task) = client.spawn();
    /// publish("sensor/temperature/1", Bytes::from("26.1"))
    ///     .emit(&handle)
    ///     .await
    ///     .unwrap();
    /// # });
    /// ```
    async fn emit(self, handler: &crate::aio::ClientHandle) -> Result<(), ConnectionError> {
        handler.send(self.into()).await?;
        Ok(())
    }
}

#[cfg(feature = "blocking")]
impl crate::blocking::Emit for Publish {
    /// Publish `payload` to the given `topic`.
    ///
    /// ```no_run
    /// use bytes::Bytes;
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{publish, Connect, blocking::{Client, Emit}, packet_identifier};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    ///
    /// publish("sensor/temperature/1", Bytes::from("26.1"))
    ///     .emit(&handle)
    ///     .unwrap();
    ///```
    fn emit(self, handler: &crate::blocking::ClientHandle) -> Result<(), ConnectionError> {
        handler.send(self.into())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_basic() {
        let packet = Publish::builder("test/topic", "Hello MQTT!")
            .qos(QoS::AtMostOnceDelivery)
            .retain(true)
            .build();

        println!("{:?}", packet.as_bytes());
        assert_eq!(packet.topic(), "test/topic");
        assert_eq!(packet.payload(), b"Hello MQTT!");
        assert_eq!(packet.qos(), QoS::AtMostOnceDelivery);
        assert!(packet.retain());
        assert!(!packet.duplicate());
        assert_eq!(packet.packet_identifier(), None);
    }

    #[test]
    fn test_publish_qos1() {
        let packet = Publish::builder("test/topic", "Hello MQTT!")
            .qos(QoS::AtLeastOnceDelivery)
            .packet_identifier(1234)
            .build();

        assert_eq!(packet.qos(), QoS::AtLeastOnceDelivery);
        assert_eq!(packet.packet_identifier(), Some(1234));
    }

    #[test]
    fn test_publish_roundtrip() {
        let original = Publish::builder("test/topic", "Hello MQTT!")
            .qos(QoS::AtLeastOnceDelivery)
            .packet_identifier(1234)
            .retain(true)
            .duplicate(true)
            .build();

        let bytes = original.clone().into_bytes();
        let decoded = Publish::try_from(bytes).unwrap();

        assert_eq!(original, decoded);
    }
}
