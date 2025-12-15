use super::decode::{packet_length, DecodingError, InvalidPacketTypeError};
use crate::{
    decode, ConnAck, Connect, Disconnect, PingReq, PingResp, PubAck, PubComp, PubRec, PubRel,
    Publish, SubAck, Subscribe, UnsubAck, Unsubscribe,
};
use bytes::Bytes;
use std::error::Error;
use std::fmt::{self, Display};

mod ack;
pub mod connack;
pub mod connect;
pub mod disconnect;
pub mod ping_req;
pub mod ping_resp;
pub mod puback;
pub mod pubcomp;
pub mod publish;
pub mod pubrec;
pub mod pubrel;
pub mod suback;
pub mod subscribe;
pub mod unsuback;
pub mod unsubscribe;

/// A model for each MQTT packet.
#[derive(Clone)]
pub enum Packet {
    /// The first message send by a client.
    Connect(Connect),

    /// A server's response on a CONNECT.
    ConnAck(ConnAck),

    /// Terminate the connection, sent the client.
    Disconnect(Disconnect),

    /// A client's way to express interest in a topic.
    Subscribe(Subscribe),

    /// A server's response to a SUBSCRIBE.
    SubAck(SubAck),

    /// Emitting a payload to a topic.
    Publish(Publish),

    /// A peer's response to a PUBLISH with QoS of 1.
    PubAck(PubAck),

    /// A peers's response to a PUBLISH with QoS of 2.
    PubComp(PubComp),

    /// A peer's response to a PUBCOMP.
    PubRec(PubRec),

    /// A peer's response to a PUBREL.
    PubRel(PubRel),

    /// Send by the client to keep the connection alive.
    PingReq(PingReq),

    /// A server's response to a PINGREQ.
    PingResp(PingResp),

    /// Used by the client to unsubscribe from a topic.
    Unsubscribe(Unsubscribe),

    /// A server's response to a UNSUBSCRIBE.
    UnsubAck(UnsubAck),
}

impl Packet {
    /// Retrieve the [`PacketType`].
    pub fn packet_type(&self) -> PacketType {
        match self {
            Self::Connect(packet) => packet.packet_type(),
            Self::ConnAck(packet) => packet.packet_type(),
            Self::Disconnect(packet) => packet.packet_type(),
            Self::Subscribe(packet) => packet.packet_type(),
            Self::SubAck(packet) => packet.packet_type(),
            Self::Publish(packet) => packet.packet_type(),
            Self::PubAck(packet) => packet.packet_type(),
            Self::PubComp(packet) => packet.packet_type(),
            Self::PubRec(packet) => packet.packet_type(),
            Self::PubRel(packet) => packet.packet_type(),
            Self::PingReq(packet) => packet.packet_type(),
            Self::PingResp(packet) => packet.packet_type(),
            Self::UnsubAck(packet) => packet.packet_type(),
            Self::Unsubscribe(packet) => packet.packet_type(),
        }
    }

    /// Serialize the packet into bytes.
    pub fn into_bytes(self) -> Bytes {
        match self {
            Self::Connect(packet) => packet.into_bytes(),
            Self::ConnAck(packet) => packet.into(),
            Self::Disconnect(packet) => packet.into(),
            Self::Subscribe(packet) => packet.into_bytes(),
            Self::SubAck(packet) => packet.into_bytes(),
            Self::Publish(packet) => packet.into_bytes(),
            Self::PubAck(packet) => packet.into(),
            Self::PubComp(packet) => packet.into(),
            Self::PubRec(packet) => packet.into(),
            Self::PubRel(packet) => packet.into(),
            Self::PingReq(packet) => packet.into(),
            Self::PingResp(packet) => packet.into(),
            Self::UnsubAck(packet) => packet.into(),
            Self::Unsubscribe(packet) => packet.into(),
        }
    }

    /// Retrieve the length of the packet in number of bytes.
    pub fn length(&self) -> usize {
        match self {
            Self::Connect(packet) => packet.length() as usize,
            Self::ConnAck(packet) => packet.length() as usize,
            Self::Disconnect(packet) => packet.length() as usize,
            Self::Subscribe(packet) => packet.length() as usize,
            Self::SubAck(packet) => packet.length() as usize,
            Self::Publish(packet) => packet.length() as usize,
            Self::PubAck(packet) => packet.length() as usize,
            Self::PubComp(packet) => packet.length() as usize,
            Self::PubRec(packet) => packet.length() as usize,
            Self::PubRel(packet) => packet.length() as usize,
            Self::PingReq(packet) => packet.length() as usize,
            Self::PingResp(packet) => packet.length() as usize,
            Self::UnsubAck(packet) => packet.length() as usize,
            Self::Unsubscribe(packet) => packet.length() as usize,
        }
    }

    /// Retrieve the payload from a packet. For packets without a payload, the
    /// length of the returned slice is 0.
    pub fn payload(&self) -> &[u8] {
        match self {
            Self::Connect(packet) => packet.payload(),
            Self::ConnAck(packet) => packet.payload(),
            Self::Disconnect(packet) => packet.payload(),
            Self::Subscribe(packet) => packet.payload(),
            Self::SubAck(packet) => packet.payload(),
            Self::Publish(packet) => packet.payload(),
            Self::PubAck(packet) => packet.payload(),
            Self::PubComp(packet) => packet.payload(),
            Self::PubRec(packet) => packet.payload(),
            Self::PubRel(packet) => packet.payload(),
            Self::PingReq(packet) => packet.payload(),
            Self::PingResp(packet) => packet.payload(),
            Self::UnsubAck(packet) => packet.payload(),
            Self::Unsubscribe(packet) => packet.payload(),
        }
    }
}

impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(packet) => packet.fmt(f),
            Self::ConnAck(packet) => packet.fmt(f),
            Self::Disconnect(packet) => packet.fmt(f),
            Self::Subscribe(packet) => packet.fmt(f),
            Self::SubAck(packet) => packet.fmt(f),
            Self::Publish(packet) => packet.fmt(f),
            Self::PubAck(packet) => packet.fmt(f),
            Self::PubComp(packet) => packet.fmt(f),
            Self::PubRec(packet) => packet.fmt(f),
            Self::PubRel(packet) => packet.fmt(f),
            Self::PingReq(packet) => packet.fmt(f),
            Self::PingResp(packet) => packet.fmt(f),
            Self::UnsubAck(packet) => packet.fmt(f),
            Self::Unsubscribe(packet) => packet.fmt(f),
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
            PacketType::Connect => Ok(Packet::Connect(Connect::try_from(value)?)),
            PacketType::PingReq => Ok(Packet::PingReq(PingReq::try_from(value)?)),
            PacketType::PingResp => Ok(Packet::PingResp(PingResp::try_from(value)?)),
            PacketType::Disconnect => Ok(Packet::Disconnect(Disconnect::try_from(value)?)),
            PacketType::PubComp => Ok(Packet::PubComp(PubComp::try_from(value)?)),
            PacketType::ConnAck => Ok(Self::ConnAck(ConnAck::try_from(value)?)),
            PacketType::SubAck => Ok(Self::SubAck(SubAck::try_from(value)?)),
            PacketType::Publish => Ok(Self::Publish(Publish::try_from(value)?)),
            PacketType::PubRec => Ok(Self::PubRec(PubRec::try_from(value)?)),
            PacketType::PubRel => Ok(Self::PubRel(PubRel::try_from(value)?)),
            PacketType::PubAck => Ok(Self::PubAck(PubAck::try_from(value)?)),
            PacketType::UnsubAck => Ok(Self::UnsubAck(UnsubAck::try_from(value)?)),
            PacketType::Unsubscribe => Ok(Self::Unsubscribe(Unsubscribe::try_from(value)?)),
            PacketType::Subscribe => Ok(Self::Subscribe(Subscribe::try_from(value)?)),
        }
    }
}

/// Every packet type of MQTT 3.1.1.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PacketType {
    /// The first message send by a client.
    Connect = 1,

    /// A server's response on a CONNECT.
    ConnAck = 2,

    /// Emitting a payload to a topic.
    Publish = 3,

    /// A peer's response to a PUBLISH with QoS of 1.
    PubAck = 4,

    /// A peers's response to a PUBLISH with QoS of 2.
    PubRec = 5,

    /// A peer's response to a PUBREC.
    PubRel = 6,

    /// A peer's response to a PUBREL.
    PubComp = 7,

    /// A client's way to express interest in a topic.
    Subscribe = 8,

    /// A server's response to a SUBSCRIBE.
    SubAck = 9,

    /// Used by the client to unsubscribe from a topic.
    Unsubscribe = 10,

    /// A server's response to a UNSUBSCRIBE.
    UnsubAck = 11,

    /// Send by the client to keep the connection alive.
    PingReq = 12,

    /// A server's response to a PINGREQ.
    PingResp = 13,

    /// Terminate the connection, sent the client.
    Disconnect = 14,
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
        };
        &inner[0..5]
    }

    /// Return the index where the variable header starts.
    /// The offset is relative to the start of the packet.
    fn offset_variable_header(&self) -> usize {
        self.header().len()
    }

    /// Return the bytes making up the variable header.
    /// The slice might be empty for packets without a variable header.
    fn variable_header(&self) -> &[u8];

    /// Return the index where the payload starts.
    /// The offset is relative to the start of the packet.
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

    /// Return the length of the frame in bytes.
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

/// The revision of the MQTT protocol.
#[repr(u8)]
pub enum ProtocolLevel {
    /// MQTT 3.1.1
    _3_1_1 = 4,
}

/// The delivery guarantee for packets [`Subscribe`] and [`Publish`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[repr(u8)]
pub enum QoS {
    /// The message is not guaranteed to be delivered.
    AtMostOnceDelivery = 0,

    /// The message arrives at once or more at the receiving end.
    AtLeastOnceDelivery = 1,

    /// The message is delivered exactly once time.
    ExactlyOnceDelivery = 2,
}

impl TryFrom<&u8> for QoS {
    type Error = InvalidQoS;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::AtMostOnceDelivery),
            1 => Ok(Self::AtLeastOnceDelivery),
            2 => Ok(Self::ExactlyOnceDelivery),
            _ => Err(InvalidQoS(*value)),
        }
    }
}

impl TryFrom<u8> for QoS {
    type Error = InvalidQoS;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        QoS::try_from(&value)
    }
}

#[derive(Debug)]
pub struct InvalidQoS(u8);

impl Error for InvalidQoS {}

impl Display for InvalidQoS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not a valid value for QoS", self.0)
    }
}

pub fn min_bytes_required(payload: &[u8]) -> u32 {
    if payload.len() < 2 {
        return 2 - payload.len() as u32;
    }

    match decode::packet_length(&payload[1..]) {
        Ok(length) => length - payload.len() as u32,
        Err(DecodingError::NotEnoughBytes { .. }) => 127 - payload.len() as u32,
        Err(error) => panic!("{error:?}"),
    }
}

// Retrieve the fixed header, variable header, and payload a frame.
// Since the frame is not verified (yet), these operations are fallible.
pub trait UnverifiedFrame {
    fn as_bytes(&self) -> &[u8];

    // Return the actual length of the frame.
    fn length(&self) -> usize {
        self.as_bytes().len()
    }

    /// Return a slice containing the fixed header.
    fn try_header(&self) -> Result<&[u8], DecodingError> {
        let inner = self.as_bytes();

        // Decode the "remaining length" field. This field is between
        // 1 and 3 bytes long and contains the number that follow _after_
        // this field.
        //
        // The length of the entire packet is:
        // * the value encoded is this field
        // * the length of this field (between 1 and 3 bytes)
        // * 1 byte for encoding the packet type
        for n in 1..5 {
            let byte = inner.get(n).ok_or(DecodingError::NotEnoughBytes {
                minimum: 1,
                actual: 0,
            })?;

            if byte & 128 == 0 {
                // TODO: Make lookup infallible.
                return Ok(&self.as_bytes()[0..n + 1]);
            }
        }

        Err(DecodingError::InvalidRemainingLength)
    }

    fn try_offset_variable_header(&self) -> Result<usize, DecodingError> {
        self.try_header().map(|header| header.len())
    }

    // Return the bytes forming the variable header.
    // The slice might be empty for packets without payload.
    fn try_variable_header(&self) -> Result<&[u8], DecodingError>;

    fn try_offset_payload(&self) -> Result<usize, DecodingError> {
        Ok(self.try_header().map(|header| header.len())?
            + self.try_variable_header().map(|header| header.len())?)
    }

    // Return the bytes forming the payload.
    // The slice might be empty for packets without payload.
    fn try_payload(&self) -> Result<&[u8], DecodingError> {
        let offset = self.try_offset_payload()?;
        let size = self.length() - offset;

        // TODO: Make lookup infallible.
        Ok(&self.as_bytes()[offset..offset + size])
    }
}
