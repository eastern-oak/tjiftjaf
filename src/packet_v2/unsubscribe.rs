//! Providing [`Subscribe`], used by client to express interest in one or more topics.
use crate::{
    Frame, Packet, PacketType, QoS,
    decode::{self, DecodingError},
    encode, packet_identifier,
    packet_v2::UnverifiedFrame,
};
use bytes::{BufMut, Bytes, BytesMut};

/// [Subscribe](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718063) allows a client to express interest in one or more topics.
///
/// # Example
///
/// Use a [`Builder`] to construct `Subscribe`.
/// ```
/// use tjiftjaf::QoS;
/// use tjiftjaf::packet_v2::subscribe::Subscribe;
///
/// let subscribe = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
///     .add_topic("topic-2", QoS::AtMostOnceDelivery)
///     .build();
/// let mut topics = subscribe.topics();
/// assert_eq!(topics.next(), Some(("topic-1", QoS::AtMostOnceDelivery)));
/// assert_eq!(topics.next(), Some(("topic-2", QoS::AtMostOnceDelivery)));
/// assert_eq!(topics.next(), None);
/// ```
///
/// Alternatively, try decoding [`Bytes`] as `Subscribe`.
/// ```
/// use tjiftjaf::QoS;
/// use tjiftjaf::packet_v2::subscribe::Subscribe;
/// use bytes::Bytes;
///
/// let frame = Bytes::copy_from_slice(&[130, 12, 75, 66, 0, 7, 116, 111, 112, 105, 99, 45, 49, 0]);
/// let packet = Subscribe::try_from(frame).unwrap();
/// assert_eq!(packet.packet_identifier(), 19266);
/// assert_eq!(packet.topics().next(), Some(("topic-1", QoS::AtMostOnceDelivery)));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Unsubscribe {
    inner: UnverifiedUnsubscribe,
}

impl Unsubscribe {
    /// Serialize `Unsubscribe`.
    pub fn into_bytes(self) -> Bytes {
        self.inner.inner
    }

    /// Creates a [`Builder`] to configure `Unsubscribe`.
    pub fn builder(topic: impl Into<String>) -> Builder {
        Builder::new(topic)
    }

    /// Retrieve the packet identifier.
    pub fn packet_identifier(&self) -> u16 {
        self.inner.try_packet_identifier().unwrap()
    }

    /// Returns an iterator over the topics.
    ///
    /// # Example
    ///
    /// ```
    /// use tjiftjaf::QoS;
    /// use tjiftjaf::packet_v2::subscribe::Subscribe;
    ///
    /// let subscribe = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
    ///     .add_topic("topic-2", QoS::AtMostOnceDelivery)
    ///     .build();
    /// let mut topics = subscribe.topics();
    /// assert_eq!(topics.next(), Some(("topic-1", QoS::AtMostOnceDelivery)));
    /// assert_eq!(topics.next(), Some(("topic-2", QoS::AtMostOnceDelivery)));
    /// assert_eq!(topics.next(), None);
    /// ```
    pub fn topics(&self) -> Topics<'_> {
        Topics {
            topics: self.payload(),
            offset: 0,
        }
    }
}

impl Frame for Unsubscribe {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        &self.as_bytes()[offset..offset + 2]
    }
}

impl TryFrom<Bytes> for Unsubscribe {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        UnverifiedUnsubscribe { inner: value }.verify()
    }
}

impl From<Unsubscribe> for Bytes {
    fn from(value: Unsubscribe) -> Bytes {
        value.inner.inner
    }
}

impl From<Unsubscribe> for Packet {
    fn from(value: Unsubscribe) -> Packet {
        Packet::Unsubscribe(value)
    }
}

impl std::fmt::Debug for Unsubscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UNSUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .field("topics", &self.topics())
            .finish()
    }
}

// TODO: implement debug manually to print topics
#[derive(Debug)]
pub struct Topics<'a> {
    topics: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for Topics<'a> {
    type Item = (&'a str, QoS);

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.topics.len() {
            return None;
        }

        let (topic, offset) = decode::field::utf8(&self.topics[self.offset..]).expect("Failed to extract topic. This should never happen, because `Topics` can only be created from a valid payload. Please report a bug.");
        self.offset += offset;
        let qos = QoS::try_from(self.topics[self.offset]).expect("Failed to extract QoS. This should never happen, because `Topics` can only be created from a valid payload. Please report a bug.");
        self.offset += 1;
        Some((topic, qos))
    }
}

#[derive(Clone, PartialEq, Eq)]
struct UnverifiedUnsubscribe {
    pub inner: Bytes,
}

impl UnverifiedUnsubscribe {
    fn try_packet_identifier(&self) -> Result<u16, DecodingError> {
        let header = self.try_variable_header()?;
        decode::u16(&header)
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != crate::PacketType::Unsubscribe {
            //  TODO return  correct packet type
            return Err(DecodingError::InvalidPacketType(10));
        }

        let packet_length = decode::packet_length(&header[1..header.len()])? as usize;
        if packet_length != self.length() {
            // TODO: Return  correct error
            return Err(DecodingError::TooManyBytes);
        }

        Ok(())
    }

    // TODO: figure out if returning `Topics` is better.
    fn try_topics(&self) -> Result<Vec<String>, DecodingError> {
        let payload = self.try_payload()?;
        let mut offset = 0;
        let mut topics = vec![];

        loop {
            let (topic, length) = decode::field::utf8(&payload[offset..])?;
            offset += length;
            topics.push(topic.to_string());

            if offset >= payload.len() {
                break;
            }
        }
        Ok(topics)
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        self.try_variable_header()?;
        Ok(())
    }

    fn verify_payload(&self) -> Result<(), DecodingError> {
        self.try_topics()?;

        // TODO: check that payload is not empty
        Ok(())
    }

    fn verify(self) -> Result<Unsubscribe, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;
        self.verify_payload()?;

        Ok(Unsubscribe { inner: self })
    }
}

impl UnverifiedFrame for UnverifiedUnsubscribe {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        // The variable header of a UNSUBSCRIBE packet has a fixed size of 2 bytes.
        let offset = self.try_offset_variable_header()?;
        Ok(&self.as_bytes()[offset..offset + 2])
    }
}

pub struct Builder {
    packet_identifier: u16,
    topics: Vec<String>,
}

impl Builder {
    pub fn new(topic: impl Into<String>) -> Self {
        let this = Self {
            packet_identifier: packet_identifier(),
            topics: vec![],
        };

        this.add_topic(topic)
    }

    pub fn add_topic(mut self, topic: impl Into<String>) -> Self {
        self.topics.push(topic.into());
        self
    }

    pub fn build(self) -> Unsubscribe {
        let mut variable_header = BytesMut::with_capacity(2);
        variable_header.put_u16(self.packet_identifier);

        let mut payload = BytesMut::new();
        for topic in self.topics {
            payload.put(encode::utf8(topic));
        }

        let mut packet = BytesMut::new();
        let packet_type: u8 = PacketType::Unsubscribe.into();
        packet.put_u8((packet_type << 4) + 2);

        let remaining_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.put(remaining_length);
        packet.put(variable_header);
        packet.put(payload);

        UnverifiedUnsubscribe {
            inner: packet.freeze(),
        }
        .verify()
        .unwrap()
    }

    pub fn build_packet(self) -> Packet {
        Packet::Unsubscribe(self.build())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subscribe() {
        let frame = Unsubscribe::builder("topic-1").build();
        dbg!(frame.as_bytes());
        let _: Unsubscribe = frame.into_bytes().try_into().unwrap();

        let frame = Unsubscribe::builder("topic-1").add_topic("topic-2").build();
        let _: Unsubscribe = frame.into_bytes().try_into().unwrap();
    }
}
