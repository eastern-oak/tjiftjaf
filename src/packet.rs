use super::decode::{self, DecodingError, InvalidPacketTypeError, packet_length};
use super::encode;
use crate::packet_v2;
use bytes::{BufMut, Bytes, BytesMut};
use std::io::Read;
use std::{fmt, u16};

#[derive(Clone)]
pub enum Packet {
    Connect(packet_v2::connect::Connect),
    ConnAck(ConnAck),
    Subscribe(Subscribe),
    SubAck(SubAck),
    Publish(Publish),
    PubAck(PubAck),
    PingReq(PingReq),
    PingResp(PingResp),
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
            Self::ConnAck(packet) => packet.inner,
            Self::Subscribe(packet) => packet.inner,
            Self::SubAck(packet) => packet.inner,
            Self::Publish(packet) => packet.inner,
            Self::PubAck(packet) => packet.inner,
            Self::PingReq(packet) => packet.inner,
            Self::PingResp(packet) => packet.inner,
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
            .get(0)
            .ok_or(DecodingError::NotEnoughBytes {
                minimum: 2,
                actual: 0,
            })?
            .try_into()?;

        match packet_type {
            PacketType::PingReq => return Ok(Packet::PingReq(PingReq { inner: value })),
            PacketType::PingResp => return Ok(Packet::PingResp(PingResp { inner: value })),

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
                return Ok(Self::ConnAck(ConnAck::new(value)));
            }
            PacketType::SubAck => {
                return Ok(Self::SubAck(SubAck::new(value)));
            }
            PacketType::Publish => return Ok(Self::Publish(Publish::from(value))),
            PacketType::PubAck => {
                return Ok(Self::PubAck(PubAck::new(value)));
            }
            _ => match value.len() {
                0..=4 => {
                    return Err(DecodingError::NotEnoughBytes {
                        minimum: 5,
                        actual: value.len(),
                    });
                }
                _ => {}
            },
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
        Self::try_from(value).expect(&format!("{} is not a valid MQTT packet type.", value))
    }
}

impl Into<u8> for PacketType {
    fn into(self) -> u8 {
        match self {
            Self::Connect => 1,
            Self::ConnAck => 2,
            Self::Publish => 3,
            Self::PubAck => 4,
            Self::PubRec => 5,
            Self::PubRel => 6,
            Self::PubComp => 7,
            Self::Subscribe => 8,
            Self::SubAck => 9,
            Self::Unsubscribe => 10,
            Self::UnsubAck => 11,
            Self::PingReq => 12,
            Self::PingResp => 13,
            Self::Disconnect => 14,
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

#[derive(Clone)]
pub struct ConnAck {
    inner: Bytes,
}

impl Frame for ConnAck {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        // This packet has a fixed length of 4 byts.
        &self.as_bytes()[2..4]
    }
}

impl ConnAck {
    pub fn new(inner: Bytes) -> Self {
        assert_eq!(PacketType::from_unchecked(inner[0]), PacketType::ConnAck);
        Self { inner }
    }

    // Whether this session is new
    pub fn session_present(&self) -> bool {
        if self.variable_header()[0] & 0x1 == 1 {
            return true;
        }

        false
    }

    pub fn return_code(&self) -> ConnectReturnCode {
        ConnectReturnCode::try_from(&self.variable_header()[1]).unwrap()
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConnectReturnCode {
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

impl TryFrom<&u8> for ConnectReturnCode {
    type Error = ();

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        let value = match value {
            0x0 => Self::ConnectionAccepted,
            0x1 => Self::ConnectionRefusedUnacceptableProtocolVersion,
            0x2 => Self::ConnectionRefusedIdentifierRejected,
            0x3 => Self::ConnectionRefusedServerUnavailable,
            0x4 => Self::ConnectionRefusedBadUsernameOrPassword,
            0x5 => Self::ConnectionRefusedNotAuthorized,
            _ => {
                eprintln!("{} is an invalid return code", value);
                return Err(());
            }
        };

        Ok(value)
    }
}

#[derive(Clone)]
pub struct Subscribe {
    inner: Bytes,
}

impl Frame for Subscribe {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        let offset = self.header().len();
        &self.as_bytes()[offset..offset + 2]
    }
}

impl Subscribe {
    pub fn new(inner: Bytes) -> Self {
        assert_eq!(PacketType::from_unchecked(inner[0]), PacketType::Subscribe);
        Self { inner }
    }

    pub fn builder() -> SubscribeBuilder {
        SubscribeBuilder::new()
    }

    pub fn identifier(&self) -> u16 {
        let offset = self.variable_header().len();

        decode::packet_identifier(&self.inner[offset..offset + 2])
            .expect("Failed to decode packet identifier.")
    }

    pub fn topic(&self) -> &str {
        let offset: usize = self.offset_payload();

        decode::utf8(&self.inner[offset..self.inner.len()]).expect("Failed to decode topic.")
    }
}

impl std::fmt::Debug for Subscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SUBSCRIBE")
            .field("length", &self.length())
            .field("packet_identifer", &self.identifier())
            .field("topic", &self.topic())
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
            .try_into()
            .unwrap()
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
    quality_of_service: usize,
    retain: bool,
    packet_identifier: u16,

    topic: Option<String>,
    payload: Option<Bytes>,
}

impl PublishBuilder {
    pub fn new() -> Self {
        Self {
            quality_of_service: 0x00,
            retain: false,
            packet_identifier: packet_identifier(),
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

#[derive(Clone)]
pub struct PingReq {
    inner: Bytes,
}

impl PingReq {
    pub fn build() -> PingReq {
        PingReq {
            inner: Bytes::copy_from_slice(&[(PacketType::PingReq as u8) << 4, 0]),
        }
    }
}

impl Frame for PingReq {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        &[]
    }
}

impl std::fmt::Debug for PingReq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PINGREQ")
            .field("length", &self.length())
            .finish()
    }
}

#[derive(Clone)]
pub struct PingResp {
    inner: Bytes,
}

impl Frame for PingResp {
    fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    fn variable_header(&self) -> &[u8] {
        &[]
    }
}

impl std::fmt::Debug for PingResp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PINGRESP")
            .field("length", &self.length())
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
