//! Provides [`Subscribe`], a message to express interest in a topic.
//!
//! # Examples
//! For most use cases, use [`crate::subscribe`] to create a [`crate::Packet::Subscribe`] that
//! expresses interest in a single topic:
//!
//! ```
//! use tjiftjaf::subscribe;
//!
//! let packet = subscribe("temperature");
//! ```
//!
//! Alternatively, use [`SubscribeBuilder`] to have control over the packet:
//!
//! ```
//! use tjiftjaf::{
//!     QoS, packet_v2::subscribe::Subscribe
//! };
//!
//! let subscribe = Subscribe::builder()
//!     // Topic 'temperature' uses QoS::AtMostOnceDelivery.
//!    .add_topic("temperature")
//!    .add_topic_with_qos("humidity", QoS::AtLeastOnceDelivery)
//!    .build();
//!
//! for (topic, qos) in subscribe.topics() {
//!    println!("{topic} - {qos:?}");
//! }
//! ```
use super::UnverifiedFrame;
use crate::{
    Frame, Packet, PacketType, QoS,
    decode::{self, DecodingError},
    encode, packet_identifier,
};
use bytes::{BufMut, Bytes, BytesMut};

/// A MQTT message to express interest in one or more topics.
#[derive(Clone, PartialEq, Eq)]
pub struct Subscribe {
    inner: UnverifiedSubscribe,
}

impl Subscribe {
    /// Creates a `SubscribeBuilder` to configure a `Subscribe`.
    /// This is the same as `SubscribeBuilder::new()`.
    pub fn builder() -> SubscribeBuilder {
        SubscribeBuilder::new()
    }

    pub fn packet_identifier(&self) -> u16 {
        self.inner.try_packet_identifier().unwrap()
    }

    /// Get a clone of all all topics and their quality of service.
    pub fn topics(&self) -> Vec<(String, QoS)> {
        self.inner.try_topics().unwrap()
    }
}

impl Frame for Subscribe {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        self.inner.try_variable_header().unwrap()
    }
}

impl From<Subscribe> for Bytes {
    fn from(value: Subscribe) -> Self {
        Bytes::copy_from_slice(&value.inner.inner)
    }
}

impl From<Subscribe> for Packet {
    fn from(value: Subscribe) -> Self {
        Packet::Subscribe(value)
    }
}

impl TryFrom<Bytes> for Subscribe {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        UnverifiedSubscribe { inner: value }.verify()
    }
}

impl std::fmt::Debug for Subscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            // .field("return_code", &self.return_code())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct UnverifiedSubscribe {
    pub inner: Bytes,
}

impl UnverifiedSubscribe {
    pub fn try_packet_identifier(&self) -> Result<u16, DecodingError> {
        Ok(u16::from_be_bytes(
            self.try_variable_header()?[0..2].try_into().unwrap(),
        ))
    }

    // TODO: return Iterator<Item = (&str, QoS)>
    pub fn try_topics(&self) -> Result<Vec<(String, QoS)>, DecodingError> {
        let mut topics: Vec<(String, QoS)> = Vec::new();
        let mut index = 0;
        let payload = self.try_payload()?;
        loop {
            let (topic, n) = decode::field::variable_length(&payload[index..])?;
            index += n;
            let topic = std::str::from_utf8(topic)
                .map_err(|_| DecodingError::InvalidValue("Topic is not valid UTF-8".into()))?;

            let qos: QoS = payload[index].try_into().unwrap();
            index += 1;
            topics.push((topic.to_string(), qos));

            if index == payload.len() {
                return Ok(topics);
            }
        }
    }

    fn verify(self) -> Result<Subscribe, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;
        self.verify_payload()?;

        Ok(Subscribe { inner: self })
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != crate::PacketType::Subscribe {
            //  TODO return  correct packet type
            return Err(DecodingError::InvalidPacketType(8));
        }

        let packet_length = decode::packet_length(&header[1..header.len()])? as usize;
        if packet_length != self.length() {
            // TODO: Return  correct error
            return Err(DecodingError::TooManyBytes);
        }

        Ok(())
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        self.try_packet_identifier()?;

        Ok(())
    }

    fn verify_payload(&self) -> Result<(), DecodingError> {
        self.try_topics()?;
        Ok(())
    }
}

impl UnverifiedFrame for UnverifiedSubscribe {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        let offset = self.try_header()?.len();
        Ok(&self.as_bytes()[offset..offset + 2])
    }
}

/// `SubscribeBuilder` can be used to to configure a [`Subscribe`].
pub struct SubscribeBuilder {
    packet_identifier: u16,
    topics: Vec<(String, QoS)>,
}

impl SubscribeBuilder {
    /// Constructs a `SubscribeBuilder`.
    ///
    /// This is the same as [`Subscribe::builder()`].
    pub fn new() -> Self {
        Self {
            packet_identifier: packet_identifier(),
            topics: Vec::new(),
        }
    }

    /// Add a topic with [`QoS::AtMostOnceDelivery`].
    pub fn add_topic(self, topic: impl Into<String>) -> Self {
        self.add_topic_with_qos(topic, QoS::AtMostOnceDelivery)
    }

    /// Add a topic with a certain `QoS`.
    pub fn add_topic_with_qos(mut self, topic: impl Into<String>, qos: QoS) -> Self {
        self.topics.push((topic.into(), qos));
        self
    }

    /// Returns a `Subscribe`.
    ///
    /// # Panic
    ///
    /// Panics if no topic has been added.
    pub fn build(self) -> Subscribe {
        if self.topics.is_empty() {
            panic!();
        };

        let mut variable_header = BytesMut::with_capacity(2);
        variable_header.put_u16(self.packet_identifier);

        let mut payload = BytesMut::new();
        for (topic, qos) in self.topics.into_iter() {
            payload.put(encode::utf8(topic));
            payload.put_u8(qos as u8);
        }

        let mut packet = BytesMut::new();
        let x: u8 = PacketType::Subscribe.into();

        packet.put_u8((x << 4) + 2);

        let remaning_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.put(remaning_length);
        packet.put(variable_header);
        packet.put(payload);

        UnverifiedSubscribe {
            inner: packet.freeze(),
        }
        .verify()
        .unwrap()
    }

    /// Returns a [`Packet::Subscribe`].
    pub fn build_packet(self) -> Packet {
        Packet::Subscribe(self.build())
    }
}

impl Default for SubscribeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use crate::{Packet, QoS, packet::Frame, packet_v2::subscribe::Subscribe, subscribe};
    use bytes::Bytes;

    #[test]
    fn test_connect() {
        let Packet::Subscribe(packet) = subscribe("test") else {
            panic!()
        };

        let topics = packet.topics();
        let (topic, qos) = topics.first().unwrap();
        assert_eq!(topic, "test");
        assert_eq!(qos, &QoS::AtMostOnceDelivery);

        let bytes = Bytes::copy_from_slice(packet.as_bytes());
        assert!(Subscribe::try_from(bytes).is_ok());

        let packet = Subscribe::builder()
            .add_topic("topic_1".to_string())
            .add_topic_with_qos("topic_2".to_string(), QoS::ExactlyOnceDelivery)
            .build();

        assert_eq!(packet.topics().len(), 2);
    }
}
