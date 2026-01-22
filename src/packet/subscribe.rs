//! Providing [`Subscribe`], used by client to express interest in one or more topics.
use crate::{
    decode::{self, DecodingError},
    encode,
    packet::UnverifiedFrame,
    packet_identifier, ConnectionError, Frame, Packet, PacketType, QoS,
};

/// [Subscribe](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718063) allows a client to express interest in one or more topics.
///
/// # Example
///
/// Use a [`Builder`] to construct `Subscribe`.
/// ```
/// use tjiftjaf::{Subscribe, QoS};
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
/// Alternatively, try decoding some bytes as `Subscribe`.
/// ```
/// use tjiftjaf::{Subscribe, QoS};
///
/// let frame = vec![130, 12, 75, 66, 0, 7, 116, 111, 112, 105, 99, 45, 49, 0];
/// let packet = Subscribe::try_from(frame).unwrap();
/// assert_eq!(packet.packet_identifier(), 19266);
/// assert_eq!(packet.topics().next(), Some(("topic-1", QoS::AtMostOnceDelivery)));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Subscribe {
    inner: UnverifiedSubscribe,
}

impl Subscribe {
    /// Serialize `Subscribe`.
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.inner
    }

    /// Creates a [`Builder`] to configure `Subscribe`.
    pub fn builder(topic: impl Into<String>, qos: QoS) -> Builder {
        Builder::new(topic, qos)
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
    /// use tjiftjaf::{Subscribe, QoS};
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

#[cfg(feature = "async")]
impl crate::aio::Emit for Subscribe {
    /// Subscribe to a topic.
    ///
    /// ```no_run
    /// # use async_net::TcpStream;
    /// # use futures_lite::FutureExt;
    /// # use tjiftjaf::{subscribe, Connect, QoS, aio::{Emit, Client}, packet_identifier};
    /// # smol::block_on(async {
    /// # let stream = TcpStream::connect("localhost:1883").await.unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, task) = client.spawn();
    /// subscribe("sensor/temperature/1").emit(&handle).await.unwrap();
    /// while let Ok(publish) = handle.subscriptions().await {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// # });
    /// ```
    async fn emit(self, handler: &crate::aio::ClientHandle) -> Result<(), ConnectionError> {
        handler.send(self.into()).await?;
        Ok(())
    }
}

#[cfg(feature = "blocking")]
impl crate::blocking::Emit for Subscribe {
    /// Subscribe to a topic.
    ///
    /// ```no_run
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{subscribe, Connect, blocking::{Client, Emit}};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    /// subscribe("sensor/temperature/1")
    ///    .emit(&handle)
    ///    .unwrap();
    /// while let Ok(publish) = handle.publication() {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// ```
    fn emit(self, handler: &crate::blocking::ClientHandle) -> Result<(), ConnectionError> {
        handler.send(self.into())?;
        Ok(())
    }
}

impl Frame for Subscribe {
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        &self.as_bytes()[offset..offset + 2]
    }
}

impl TryFrom<Vec<u8>> for Subscribe {
    type Error = DecodingError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        UnverifiedSubscribe { inner: value }.verify()
    }
}

impl From<Subscribe> for Vec<u8> {
    fn from(value: Subscribe) -> Vec<u8> {
        value.inner.inner
    }
}

impl From<Subscribe> for Packet {
    fn from(value: Subscribe) -> Packet {
        Packet::Subscribe(value)
    }
}

impl std::fmt::Debug for Subscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = vec![];
        // let topics = self.topics();
        for (topic, _) in self.topics() {
            list.push(topic);
        }

        f.debug_struct("SUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .field("topics", &list)
            .finish()
    }
}

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
struct UnverifiedSubscribe {
    pub inner: Vec<u8>,
}

impl UnverifiedSubscribe {
    fn try_packet_identifier(&self) -> Result<u16, DecodingError> {
        let header = self.try_variable_header()?;
        decode::u16(header)
    }

    fn verify_header(&self) -> Result<(), DecodingError> {
        let header = self.try_header()?;
        let packet_type = decode::packet_type(header)?;
        if packet_type != crate::PacketType::Subscribe {
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

    // TODO: figure out if returning `Topics` is better.
    fn try_topics(&self) -> Result<Vec<(String, QoS)>, DecodingError> {
        let payload = self.try_payload()?;
        let mut offset = 0;
        let mut topics = vec![];

        loop {
            let (topic, length) = decode::field::utf8(&payload[offset..])?;
            offset += length;
            let qos = QoS::try_from(payload[offset]).map_err(|_| {
                DecodingError::InvalidValue(format!(
                    "{} is not a valid value for QoS",
                    payload[offset]
                ))
            })?;
            offset += 1;
            topics.push((topic.to_string(), qos));

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

    fn verify(self) -> Result<Subscribe, DecodingError> {
        self.verify_header()?;
        self.verify_variable_header()?;
        self.verify_payload()?;

        Ok(Subscribe { inner: self })
    }
}

impl UnverifiedFrame for UnverifiedSubscribe {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn try_variable_header(&self) -> Result<&[u8], DecodingError> {
        // The variable header of a SUBSCRIBE packet has a fixed size of 2 bytes.
        let offset = self.try_offset_variable_header()?;
        Ok(&self.as_bytes()[offset..offset + 2])
    }
}

#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary, Debug))]
pub struct Builder {
    packet_identifier: u16,
    #[cfg_attr(feature = "arbitrary", arbitrary(with = arbitrary_topics))]
    topics: Vec<(String, QoS)>,
}

#[cfg(feature = "arbitrary")]
fn arbitrary_topics(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Vec<(String, QoS)>> {
    use std::ops::ControlFlow;
    let mut topics: Vec<(String, QoS)> = vec![];
    // A `Subscribe` packet can not have more than 255 subscriptions.
    u.arbitrary_loop(Some(1), Some(255), |u| {
        topics.push(u.arbitrary()?);

        Ok(ControlFlow::Continue(()))
    })?;

    Ok(topics)
}

impl Builder {
    pub fn new(topic: impl Into<String>, qos: QoS) -> Self {
        let this = Self {
            packet_identifier: packet_identifier(),
            topics: vec![],
        };

        this.add_topic(topic, qos)
    }

    pub fn add_topic(mut self, topic: impl Into<String>, qos: QoS) -> Self {
        self.topics.push((topic.into(), qos));
        self
    }

    pub fn build(self) -> Subscribe {
        // TODO: Optimize
        // let mut variable_header = BytesMut::with_capacity(2);
        // variable_header.put_u16(self.packet_identifier);

        let mut variable_header: Vec<u8> = self.packet_identifier.to_be_bytes().to_vec();

        let mut payload = Vec::new();
        for (topic, qos) in self.topics {
            payload.append(&mut encode::utf8(topic).to_vec());
            payload.push(qos as u8);
        }

        let mut packet = Vec::new();
        let packet_type: u8 = PacketType::Subscribe.into();
        packet.push((packet_type << 4) + 2);

        let remaining_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.append(&mut remaining_length.to_vec());
        packet.append(&mut variable_header);
        packet.append(&mut payload);

        UnverifiedSubscribe { inner: packet }.verify().unwrap()
    }

    pub fn build_packet(self) -> Packet {
        Packet::Subscribe(self.build())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subscribe() {
        let frame = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery).build();
        let _: Subscribe = frame.into_bytes().try_into().unwrap();

        let frame = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
            .add_topic("topic-2", QoS::AtLeastOnceDelivery)
            .build();
        let _: Subscribe = frame.into_bytes().try_into().unwrap();
    }

    // Issue #40 tracks a bug when the `Builder` panics
    // trying to create a `Subscribe` with a lot of topics.
    //
    // This test verifies the fix works. `Builder.build()` must _not_ panic.
    #[test]
    fn gh_40_fix_panic_when_building_subscribe_with_a_lot_of_topics() {
        let mut builder = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery);
        for _ in 0..1145729 {
            builder = builder.add_topic("", QoS::AtMostOnceDelivery);
        }

        builder.build();
    }

    // Issue #45 tracks a bug when the `Subscribe.topics()` panics
    // if the message includes a lot of topics.
    //
    // This test verifies the fix works. Iterating over the topics must _not_ panic.
    #[test]
    fn gh_45_fix_panic_when_iterating_over_the_topics_of_large_subscribe() {
        let mut builder = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery);
        for _ in 0..1145729 {
            builder = builder.add_topic("", QoS::AtMostOnceDelivery);
        }

        let packet = builder.build();
        let topics = packet.topics();
        for _ in topics {}
    }
}
