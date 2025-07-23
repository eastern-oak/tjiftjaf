use super::decode::{self, DecodingError, InvalidPacketTypeError, packet_length};
use super::encode;
use crate::packet_v2;
use crate::packet_v2::connack::ConnAck;
use crate::packet_v2::connect::Connect;
use crate::packet_v2::ping_req::PingReq;
use crate::packet_v2::ping_resp::PingResp;
use bytes::{BufMut, Bytes, BytesMut};
use std::fmt;
use std::io::Read;

#[derive(Clone)]
pub enum Packet {
    Connect(packet_v2::connect::Connect),
    ConnAck(packet_v2::connack::ConnAck),
    Subscribe(packet_v2::subscribe::Subscribe),
    SubAck(SubAck),
    Publish(Publish),
    PubAck(PubAck),
    PingReq(packet_v2::ping_req::PingReq),
    PingResp(packet_v2::ping_resp::PingResp),
    Other(Bytes),
}

impl Packet {
    pub fn packet_type(&self) -> PacketType {
        match self {
            Self::Connect(packet) => packet.packet_type(),
            Self::ConnAck(packet) => packet.packet_type(),
            Self::Subscribe(packet) => packet.packet_type(),
            Self::SubAck(packet) => packet.packet_type(),
            Self::Publish(packet) => packet.packet_type(),
            Self::PubAck(packet) => packet.packet_type(),
            Self::PingReq(packet) => packet.packet_type(),
            Self::PingResp(packet) => packet.packet_type(),
            Self::Other(inner) => PacketType::from_unchecked(inner[0]),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        match self {
            Self::Connect(packet) => packet.into_bytes(),
            Self::ConnAck(packet) => packet.into(),
            Self::Subscribe(packet) => packet.into(),
            Self::SubAck(packet) => packet.inner,
            Self::Publish(packet) => packet.inner,
            Self::PubAck(packet) => packet.inner,
            Self::PingReq(packet) => packet.into(),
            Self::PingResp(packet) => packet.into(),
            Self::Other(inner) => inner,
        }
    }

    pub fn length(&self) -> usize {
        match self {
            Self::Connect(packet) => packet.length() as usize,
            Self::ConnAck(packet) => packet.length() as usize,
            Self::Subscribe(packet) => packet.length() as usize,
            Self::SubAck(packet) => packet.length() as usize,
            Self::Publish(packet) => packet.length() as usize,
            Self::PubAck(packet) => packet.length() as usize,
            Self::PingReq(packet) => packet.length() as usize,
            Self::PingResp(packet) => packet.length() as usize,
            Self::Other(inner) => inner.len(),
        }
    }

    pub fn payload(&self) -> &[u8] {
        match self {
            Self::Connect(packet) => packet.payload(),
            Self::ConnAck(packet) => packet.payload(),
            Self::Subscribe(packet) => packet.payload(),
            Self::SubAck(packet) => packet.payload(),
            Self::Publish(packet) => packet.payload(),
            Self::PubAck(packet) => packet.payload(),
            Self::PingReq(packet) => packet.payload(),
            Self::PingResp(packet) => packet.payload(),
            Self::Other(_) => unimplemented!(),
        }
    }
}

impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(packet) => packet.fmt(f),
            Self::ConnAck(packet) => packet.fmt(f),
            Self::Subscribe(packet) => packet.fmt(f),
            Self::SubAck(packet) => packet.fmt(f),
            Self::Publish(packet) => packet.fmt(f),
            Self::PubAck(packet) => packet.fmt(f),
            Self::PingReq(packet) => packet.fmt(f),
            Self::PingResp(packet) => packet.fmt(f),
            Self::Other(inner) => {
                write!(f, "{:?}", PacketType::try_from(inner[0]).unwrap())
            }
        }
    }
}

impl TryFrom<Bytes> for Packet {
    type Error = DecodingError;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        let packet_type: PacketType = value
            .first()
            .ok_or(DecodingError::NotEnoughBytes {
                minimum: 2,
                actual: 0,
            })?
            .try_into()?;

        match packet_type {
            PacketType::Connect => return Ok(Packet::Connect(Connect::try_from(value)?)),
            PacketType::PingReq => return Ok(Packet::PingReq(PingReq::try_from(value)?)),
            PacketType::PingResp => return Ok(Packet::PingResp(PingResp::try_from(value)?)),

            PacketType::Disconnect => match value.len() {
                0 | 1 => {
                    return Err(DecodingError::NotEnoughBytes {
                        minimum: 2,
                        actual: value.len(),
                    });
                }
                2 => {}
                _ => return Err(DecodingError::TooManyBytes),
            },
            PacketType::PubRec
            | PacketType::PubRel
            | PacketType::PubComp
            | PacketType::UnsubAck => match value.len() {
                0..=3 => {
                    return Err(DecodingError::NotEnoughBytes {
                        minimum: 4,
                        actual: value.len(),
                    });
                }
                4 => {}
                _ => return Err(DecodingError::TooManyBytes),
            },
            PacketType::ConnAck => {
                return Ok(Self::ConnAck(ConnAck::try_from(value)?));
            }
            PacketType::SubAck => {
                return Ok(Self::SubAck(SubAck::new(value)));
            }
            PacketType::Publish => return Ok(Self::Publish(Publish::from(value))),
            PacketType::PubAck => {
                return Ok(Self::PubAck(PubAck::new(value)));
            }
            _ => {
                if let 0..=4 = value.len() {
                    return Err(DecodingError::NotEnoughBytes {
                        minimum: 5,
                        actual: value.len(),
                    });
                }
            }
        }

        Ok(Self::Other(value))
    }
}
// An mqtt frame consists of 3 parts:
// - A fixed header, present in all MQTT Control Packets
// - A variable header, present in some
// - A payload, present in some
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PacketType {
    Connect = 1,
    ConnAck = 2,
    Publish = 3,
    PubAck = 4,
    PubRec = 5,
    PubRel = 6,
    PubComp = 7,
    Subscribe = 8,
    SubAck = 9,
    Unsubscribe = 10,
    UnsubAck = 11,
    PingReq = 12,
    PingResp = 13,
    Disconnect = 14,
}

impl PacketType {
    fn from_unchecked(value: u8) -> Self {
        Self::try_from(value).unwrap_or_else(|_| panic!("{value} is not a valid MQTT packet type."))
    }
}

impl From<PacketType> for u8 {
    fn from(value: PacketType) -> u8 {
        match value {
            PacketType::Connect => 1,
            PacketType::ConnAck => 2,
            PacketType::Publish => 3,
            PacketType::PubAck => 4,
            PacketType::PubRec => 5,
            PacketType::PubRel => 6,
            PacketType::PubComp => 7,
            PacketType::Subscribe => 8,
            PacketType::SubAck => 9,
            PacketType::Unsubscribe => 10,
            PacketType::UnsubAck => 11,
            PacketType::PingReq => 12,
            PacketType::PingResp => 13,
            PacketType::Disconnect => 14,
        }
    }
}

impl TryFrom<&u8> for PacketType {
    type Error = InvalidPacketTypeError;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        let packet_type = match value >> 4 {
            1 => Self::Connect,
            2 => Self::ConnAck,
            3 => Self::Publish,
            4 => Self::PubAck,
            5 => Self::PubRec,
            6 => Self::PubRel,
            7 => Self::PubComp,
            8 => Self::Subscribe,
            9 => Self::SubAck,
            10 => Self::Unsubscribe,
            11 => Self::UnsubAck,
            12 => Self::PingReq,
            13 => Self::PingResp,
            14 => Self::Disconnect,
            // TODO: does this count as zero-copy?
            _ => return Err(InvalidPacketTypeError(*value)),
        };

        Ok(packet_type)
    }
}

impl TryFrom<u8> for PacketType {
    type Error = InvalidPacketTypeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

pub trait Frame {
    fn as_bytes(&self) -> &[u8];

    /// Return the bytes forming the header.
    fn header(&self) -> &[u8] {
        let inner = self.as_bytes();

        if inner[1] & 128 == 0 {
            return &self.as_bytes()[0..2];
        } else if inner[2] & 128 == 0 {
            return &inner[0..3];
        } else if inner[3] & 128 == 0 {
            return &inner[0..4];
        }

        panic!("Illegal packet")
    }

    fn offset_variable_header(&self) -> usize {
        self.header().len()
    }
    // Return the bytes forming the variable header.
    // The slice might be empty for packets without payload.
    fn variable_header(&self) -> &[u8];

    fn offset_payload(&self) -> usize {
        self.header().len() + self.variable_header().len()
    }

    // Return the bytes forming the payload.
    // The slice might be empty for packets without payload.
    fn payload(&self) -> &[u8] {
        let offset = self.header().len() + self.variable_header().len();
        let size = self.length() as usize - offset;

        &self.as_bytes()[offset..offset + size]
    }

    // Return the length of the frame in bytes.
    fn length(&self) -> u32 {
        let inner = self.as_bytes();

        packet_length(&inner[1..inner.len()]).unwrap()
    }

    fn packet_type(&self) -> PacketType {
        assert!(
            self.as_bytes().len() >= 2,
            "Frame is not long enough. It should at be 2 byts at minimum."
        );

        PacketType::try_from(&self.as_bytes()[0]).expect("Failed to decode packet type")
    }
}

#[repr(u8)]
pub enum ProtocolLevel {
    _3_1_1 = 4,
}

#[derive(Debug)]
#[repr(u8)]
pub enum QoS {
    AtMostOnceDelivery = 0,
    AtLeastOnceDelivery = 1,
    ExactlyOnceDelivery = 2,
}

impl TryFrom<u8> for QoS {
    type Error = DecodingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        QoS::try_from(&value)
    }
}

impl TryFrom<&u8> for QoS {
    type Error = DecodingError;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::AtMostOnceDelivery),
            1 => Ok(Self::AtLeastOnceDelivery),
            2 => Ok(Self::ExactlyOnceDelivery),
            _ => Err(DecodingError::Other),
        }
    }
}

#[derive(Clone)]
pub struct SubAck {
    inner: Bytes,
}

impl Frame for SubAck {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        &self.as_bytes()[offset..offset + 2]
    }
}

impl SubAck {
    pub fn new(inner: Bytes) -> Self {
        assert_eq!(PacketType::from_unchecked(inner[0]), PacketType::SubAck);
        Self { inner }
    }

    pub fn identifier(&self) -> u16 {
        u16::from_be_bytes(self.variable_header().try_into().unwrap())
    }
}

impl std::fmt::Debug for SubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SUBACK")
            .field("length", &self.length())
            .field("packet_identifer", &self.identifier())
            .finish()
    }
}

#[derive(Clone)]
pub struct Publish {
    inner: Bytes,
}

impl Frame for Publish {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        let size = self.topic().len() + 2;
        &self.inner[offset..offset + size]
    }
}

impl Publish {
    pub fn builder() -> PublishBuilder {
        PublishBuilder::new()
    }

    pub fn from(inner: Bytes) -> Self {
        assert_eq!(PacketType::from_unchecked(inner[0]), PacketType::Publish);
        Self { inner }
    }

    pub fn topic(&self) -> &str {
        let offset: usize = self.offset_variable_header();
        decode::utf8(&self.inner[offset..self.inner.len()]).expect("Failed to decode topic.")
    }
}

impl std::fmt::Debug for Publish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBLISH")
            .field("length", &self.length())
            .field("topic", &self.topic())
            .finish()
    }
}

pub struct PublishBuilder {
    topic: Option<String>,
    payload: Option<Bytes>,
}

impl PublishBuilder {
    pub fn new() -> Self {
        Self {
            topic: None,
            payload: None,
        }
    }

    pub fn topic(mut self, topic: String) -> Self {
        self.topic = Some(topic);
        self
    }

    pub fn payload(mut self, payload: Bytes) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn build(self) -> Publish {
        if self.topic.is_none() || self.payload.is_none() {
            panic!("Topic and/or payload is not set.");
        }

        let topic = self.topic.unwrap();
        let payload = self.payload.unwrap();

        let variable_header = encode::utf8(topic);
        let remaning_length = encode::remaining_length(variable_header.len() + payload.len());

        let mut packet = BytesMut::new();

        let x: u8 = PacketType::Publish.into();

        packet.put_u8(x << 4);
        packet.put(remaning_length);
        packet.put(variable_header);
        packet.put(payload);

        Publish {
            inner: packet.freeze(),
        }
    }

    pub fn build_packet(self) -> Packet {
        Packet::Publish(self.build())
    }
}

impl Default for PublishBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct PubAck {
    inner: Bytes,
}

impl PubAck {
    pub fn new(inner: Bytes) -> Self {
        assert_eq!(inner.len(), 4);
        assert_eq!(PacketType::from_unchecked(inner[0]), PacketType::Publish);
        Self { inner }
    }

    pub const fn packet_type(&self) -> PacketType {
        PacketType::PubAck
    }

    // Return the length of the packet.
    fn length(&self) -> u32 {
        4
    }

    pub fn identifier(&self) -> u16 {
        u16::from_be_bytes(self.inner[3..5].try_into().unwrap())
    }
}

impl Frame for PubAck {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        &self.inner[2..4]
    }
}

impl std::fmt::Debug for PubAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PUBACK")
            .field("length", &self.length())
            .field("packet_identifier", &self.identifier())
            .finish()
    }
}

pub fn packet_identifier() -> u16 {
    let mut buf = vec![0u8, 2];
    std::fs::File::open("/dev/urandom")
        .expect("Failed to open /dev/urandom.")
        .read_exact(&mut buf)
        .expect("Failed to obtain random data.");

    (buf[0] as u16 * 256) + buf[1] as u16
}
