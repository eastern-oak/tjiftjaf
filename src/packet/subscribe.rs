//! Providing [`Subscribe`], used by client to express interest in one or more topics.
use crate::{
    decode::{self, DecodingError},
    encode,
    packet::{BuilderError, UnverifiedFrame},
    packet_identifier, ConnectionError, Filter, Frame, Packet, PacketType, QoS,
};

/// [Subscribe](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718063) allows a client to express interest in one or more topics.
///
/// # Example
///
/// Use a [`Builder`] to construct `Subscribe`.
/// ```
/// use tjiftjaf::{Subscribe, QoS, Filter};
///
/// let subscribe = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
///     .add_filter("topic-2", QoS::AtMostOnceDelivery)
///     .build()
///     .unwrap();
/// assert_eq!(subscribe.filters(),
///     vec![
///         (Filter::new("topic-1").unwrap(), QoS::AtMostOnceDelivery),
///         (Filter::new("topic-2").unwrap(), QoS::AtMostOnceDelivery),
///     ]
/// );
/// ```
///
/// Alternatively, try decoding some bytes as `Subscribe`.
/// ```
/// use tjiftjaf::{Subscribe, QoS, Filter};
///
/// let frame = vec![130, 12, 75, 66, 0, 7, 116, 111, 112, 105, 99, 45, 49, 0];
/// let packet = Subscribe::try_from(frame).unwrap();
/// assert_eq!(packet.packet_identifier(), 19266);
/// assert_eq!(packet.filters(), vec![(Filter::new("topic-1").unwrap(), QoS::AtMostOnceDelivery)]);
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
    /// use tjiftjaf::{Subscribe, QoS, Filter};
    ///
    /// let subscribe = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
    ///     .add_filter("topic-2", QoS::AtMostOnceDelivery)
    ///     .build()
    ///     .unwrap();
    /// assert_eq!(subscribe.filters(),
    ///     vec![
    ///         (Filter::new("topic-1").unwrap(), QoS::AtMostOnceDelivery),
    ///         (Filter::new("topic-2").unwrap(), QoS::AtMostOnceDelivery),
    ///     ]
    /// );
    /// ```
    pub fn filters(&self) -> Vec<(Filter<'_>, QoS)> {
        // One can not create an illegal `Subscribe` packet. Therefore,
        // this `unwrap()` never panics.
        self.inner.try_filters().unwrap()
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
    /// # let connect = Connect::builder().build().unwrap();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, task) = client.spawn();
    /// subscribe("sensor/temperature/1").unwrap().emit(&handle).await.unwrap();
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
    /// subscribe("sensor/temperature/1").unwrap()
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
        for (topic, _) in self.filters() {
            list.push(topic);
        }

        f.debug_struct("SUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifier", &self.packet_identifier())
            .field("topics", &list)
            .finish()
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
        if self.try_flags()? != 0b0010 {
            return Err(DecodingError::HeaderContainsInvalidFlags);
        }
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

    fn try_filters(&self) -> Result<Vec<(Filter<'_>, QoS)>, DecodingError> {
        let mut filters = Vec::new();
        let mut payload = self.try_payload()?;

        while !payload.is_empty() {
            let (filter, offset) = decode::field::utf8(payload)?;
            let filter = Filter::new(filter)?;
            let qos = QoS::try_from(payload[offset]).unwrap();
            filters.push((filter, qos));

            payload = &payload[offset + 1..];
        }
        // [MQTT-3.8.3-3] The payload of a SUBSCRIBE packet MUST contain  at least one Topic Filter / QoS pair.
        if filters.is_empty() {
            return Err(DecodingError::Other);
        }
        Ok(filters)
    }

    fn verify_variable_header(&self) -> Result<(), DecodingError> {
        self.try_variable_header()?;
        Ok(())
    }

    fn verify_payload(&self) -> Result<(), DecodingError> {
        self.try_filters()?;

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
    filters: Vec<(String, QoS)>,
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
            filters: vec![],
        };

        this.add_filter(topic, qos)
    }

    pub fn add_filter(mut self, filter: impl Into<String>, qos: QoS) -> Self {
        self.filters.push((filter.into(), qos));
        self
    }

    pub fn build(self) -> Result<Subscribe, BuilderError> {
        let mut variable_header: Vec<u8> = self.packet_identifier.to_be_bytes().to_vec();

        let mut payload = Vec::new();
        for (filter, qos) in self.filters {
            Filter::new(&filter)?;
            payload.append(&mut encode::utf8(filter)?.to_vec());
            payload.push(qos as u8);
        }

        let mut packet = Vec::new();
        let packet_type: u8 = PacketType::Subscribe.into();

        // Set the second flag, that is required as per specification.
        packet.push((packet_type << 4) | 0b0010);

        let remaining_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.append(&mut remaining_length.to_vec());
        packet.append(&mut variable_header);
        packet.append(&mut payload);

        assert!(UnverifiedSubscribe {
            inner: packet.clone()
        }
        .verify()
        .is_ok());
        Ok(Subscribe {
            inner: UnverifiedSubscribe { inner: packet },
        })
    }

    pub fn build_packet(self) -> Result<Packet, BuilderError> {
        Ok(Packet::Subscribe(self.build()?))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_subscribe() {
        let frame = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
            .build()
            .unwrap();
        let _: Subscribe = frame.into_bytes().try_into().unwrap();

        let frame = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery)
            .add_filter("topic-2", QoS::AtLeastOnceDelivery)
            .build()
            .unwrap();
        let _: Subscribe = frame.into_bytes().try_into().unwrap();
    }

    // Issue #40 tracks a bug when the `Builder` panics
    // trying to create a `Subscribe` with a lot of topics.
    //
    // This test verifies the fix works. `Builder.build()` must _not_ panic.
    #[test]
    fn gh_40_fix_panic_when_building_subscribe_with_a_lot_of_topics() {
        let mut builder = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery);
        for n in 0..1145729 {
            builder = builder.add_filter(format!("topic-{n}"), QoS::AtMostOnceDelivery);
        }

        builder.build().unwrap();
    }

    // Issue #45 tracks a bug when the `Subscribe.topics()` panics
    // if the message includes a lot of topics.
    //
    // This test verifies the fix works. Iterating over the topics must _not_ panic.
    #[test]
    fn gh_45_fix_panic_when_iterating_over_the_topics_of_large_subscribe() {
        let mut builder = Subscribe::builder("topic-1", QoS::AtMostOnceDelivery);
        for n in 0..1145729 {
            builder = builder.add_filter(format!("topic-{n}"), QoS::AtMostOnceDelivery);
        }

        let packet = builder.build().unwrap();
        let topics = packet.filters();
        for _ in topics {}
    }
    /// #105 tracks a bug the parser didn't verify a message's flags.
    /// As result, the parser would happily parse a message with incorrect
    /// flags. This test verifies that the parser now fails.
    #[test]
    fn test_gh_105_fix_parsing_incorrect_flags() {
        let mut packet = Subscribe::builder("sensor/1", QoS::AtMostOnceDelivery)
            .build()
            .unwrap()
            .as_bytes()
            .to_vec();
        assert!(Subscribe::try_from(packet.clone()).is_ok());

        // The flags are configured in the first byte of the message.
        // This line changes the flags to an illegal value.
        packet[0] |= 0b0001;
        assert_eq!(
            Subscribe::try_from(packet).unwrap_err(),
            DecodingError::HeaderContainsInvalidFlags
        );
    }
}
