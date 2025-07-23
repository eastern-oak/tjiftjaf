use crate::{
    Frame, Packet, PacketType, QoS,
    decode::{self, DecodingError},
    encode, packet_identifier,
};
use bytes::{BufMut, Bytes, BytesMut};

#[derive(Clone, PartialEq, Eq)]
pub struct Subscribe {
    inner: Bytes,
}

impl Subscribe {
    pub fn builder() -> SubscribeBuilder {
        SubscribeBuilder::new()
    }

    pub fn packet_identifier(&self) -> u16 {
        decode::u16(self.variable_header()).unwrap()
    }

    pub fn first_topic(&self) -> (&str, QoS) {
        let payload = self.payload();
        let (topic, offset) = decode::field::utf8(payload).unwrap();

        let qos = QoS::try_from(&payload[offset]).unwrap();
        (topic, qos)
    }

    pub fn topics(&self) -> Vec<(&str, QoS)> {
        let payload = self.payload();
        let mut offset = 0;
        let mut topics = Vec::new();

        loop {
            // TODO: this can panic  if offset isn't  correctly encoded
            let (topic, n) = decode::field::utf8(&payload[offset..]).unwrap();
            offset += n;
            let qos = QoS::try_from(payload[offset]).unwrap();
            offset += 1;

            topics.push((topic, qos));

            if offset == payload.len() {
                break;
            }
        }

        topics
    }
}

impl Frame for Subscribe {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        &self.as_bytes()[2..4]
    }
}

impl From<Subscribe> for Bytes {
    fn from(value: Subscribe) -> Self {
        Bytes::copy_from_slice(&value.inner)
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
        // The packet is at minimum 7 bytes long:
        // * 2, 3, or 4 byte header,
        // * 2 byte identifier
        // * 4 or more byte topic (2 for length,  1 ir more for topic, 1 for qos)
        if value.len() <= 8 {
            return Err(DecodingError::NotEnoughBytes {
                minimum: 8,
                actual: value.len(),
            });
        }

        if value[0] != 128 {
            return Err(DecodingError::InvalidPacketType(value[0]));
        };

        let length = decode::packet_length(&value[1..])?;
        if length as usize != value.len() {
            return Err(DecodingError::InvalidRemainingLength);
        }
        let this = Subscribe { inner: value };
        let payload = this.payload();

        let mut offset = 0;

        loop {
            // TODO: this can panic  if offset isn't  correctly encoded
            let (_, n) = decode::field::utf8(&payload[offset..])?;
            offset += n;
            let _ = QoS::try_from(payload[offset])?;
            offset += 1;

            if offset == payload.len() {
                break;
            }
        }

        Ok(this)
    }
}

impl std::fmt::Debug for Subscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifer", &self.packet_identifier())
            .field("topic", &self.topics())
            .finish()
    }
}
pub struct SubscribeBuilder {
    packet_identifier: u16,
    topic: Option<String>,
}

impl SubscribeBuilder {
    pub fn new() -> Self {
        Self {
            packet_identifier: packet_identifier(),
            topic: None,
        }
    }

    pub fn add_topic(mut self, topic: String) -> Self {
        self.topic = Some(topic);
        self
    }

    pub fn build(self) -> Subscribe {
        let Some(topic) = self.topic else {
            panic!();
        };

        let mut variable_header = BytesMut::with_capacity(2);
        variable_header.put_u16(self.packet_identifier);

        // TODO: Add support for subscribing to multiple topics.
        let mut payload = BytesMut::with_capacity(topic.len() + 3);

        payload.put(encode::utf8(topic));
        payload.put_u8(QoS::AtMostOnceDelivery as u8);

        let mut packet = BytesMut::new();

        let x: u8 = PacketType::Subscribe.into();

        packet.put_u8((x << 4) + 2);

        let remaning_length = encode::remaining_length(variable_header.len() + payload.len());
        packet.put(remaning_length);
        packet.put(variable_header);
        packet.put(payload);

        Subscribe {
            inner: packet.freeze(),
        }
    }

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
    #[test]
    fn test() {}
}
