#![doc = include_str!("../README.md")]
#[doc(inline)]
pub use crate::decode::DecodingError;
#[doc(inline)]
pub use crate::packet::{
    connack::ConnAck, connect::Connect, disconnect::Disconnect, ping_req::PingReq,
    ping_resp::PingResp, puback::PubAck, pubcomp::PubComp, publish::Publish, pubrec::PubRec,
    pubrel::PubRel, suback::SubAck, subscribe::Subscribe, unsuback::UnsubAck,
    unsubscribe::Unsubscribe, Frame, Packet, PacketType, ProtocolLevel, QoS,
};
use bytes::{BufMut, Bytes, BytesMut};
use log::{debug, error, trace};
use std::{
    error::Error,
    fmt::Display,
    time::{Duration, Instant, SystemTime},
};

mod client;
pub mod decode;
mod encode;
pub mod packet;
mod validate;

#[cfg(feature = "blocking")]
pub mod blocking;

#[cfg(feature = "async")]
pub mod aio;

pub fn packet_identifier() -> u16 {
    let seconds = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_nanos(),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    };

    seconds as u16
}

pub fn connect(client_id: String, keep_alive_interval: u16) -> Packet {
    Connect::builder()
        .client_id(client_id)
        .keep_alive(keep_alive_interval)
        .build_packet()
}

/// Construct a [`Subscribe`] with the given topic and [`QoS::AtMostOnceDelivery`].
///
/// It is analogous to:
///
/// ```
/// use tjiftjaf::{Subscribe, QoS};
///
/// let topic = "sensor/1/#";
/// Subscribe::builder(topic, QoS::AtMostOnceDelivery).build();
/// ```
pub fn subscribe(topic: &str) -> Subscribe {
    Subscribe::builder(topic, QoS::AtMostOnceDelivery).build()
}

/// Construct a [`Unsubscribe`] with the given topic.
///
/// It is analogous to:
///
/// ```
/// use tjiftjaf::Unsubscribe;
///
/// let topic = "sensor/1/#";
/// Unsubscribe::builder(topic).build();
/// ```
pub fn unsubscribe(topic: &str) -> Unsubscribe {
    Unsubscribe::builder(topic).build()
}

/// Construct a [`Publish`] with the given topic and payload.
///
/// The flags for QoS, retain and duplicate are all 0.
///
/// It is analogous to:
///
/// ```
/// use bytes::Bytes;
/// use tjiftjaf::Publish;
///
/// let topic = "sensor/1/#";
/// let payload = Bytes::from("26.1");
/// Publish::builder(topic, payload).build();
/// ```
pub fn publish(topic: &str, payload: Bytes) -> Publish {
    Publish::builder(topic, payload).build()
}

#[derive(Default, Debug)]
enum State {
    // The state machine is waiting for the start of a new packet.
    #[default]
    StartOfHeader,

    // The state machine processed first half of a header and is waiting
    // for the remainder of the header.
    EndOfHeader {
        partial_header: Bytes,
    },

    // The state machine processed the header and it knows the length
    // of the entire packet. Now it waits for the remaining bytes
    // to complete the packet.
    RestOfPacket {
        header: Bytes,
        // The number of bytes that are pending.
        bytes_remaining: u32,
    },
}

pub struct MqttBinding {
    connection_status: ConnectionStatus,
    state: State,
    transmits: Vec<Packet>,

    statistics: Statistics,

    last_io: Instant,
    connect: Connect,
}

// The driver must do 2 things:
// * request a buffer, it'll need to read bytes from the socket and fill the buffer until it's fill.
// * request a buffer to write,
impl MqttBinding {
    pub fn from_connect(connect: Connect) -> Self {
        Self {
            connection_status: ConnectionStatus::default(),
            state: State::default(),
            transmits: vec![],
            statistics: Statistics::default(),
            last_io: Instant::now(),
            connect,
        }
    }

    pub fn handle_timeout(&mut self, now: Instant) {
        if (now - self.last_io).as_secs() >= self.connect.keep_alive() as u64 {
            // Always schedule a PINGREQ request, even if `self.keep_alive()` is 0.
            // That is against the specification. However, when this value is 0 seconds,
            // `MqttBinding.poll_timeout()` returns an value 30 years from now.
            //
            // So if keep_alive is 0 _and_ there is no IO for 30 years, then the binding
            // violates the spec by emitting a PINGREQ.
            self.transmits.push(Packet::PingReq(PingReq))
        }
    }

    pub fn poll_timeout(&mut self) -> Instant {
        let mut interval = self.connect.keep_alive() as u64;
        if interval == 0 {
            // If keep_alive() interval is 0 seconds, the client is not supposed
            // to emit PINGREQ requests. Therefore, binding does not have to be woken up
            // X seconds after the last IO to schedule a PINGREQ.
            //
            // Unfortunately, there is not a way to obtain the maximum value of `Instant`.
            // For example,  `Instant::MAX` does not exists. So we return an `Instant`
            // roughly 30 years from now. It is inspired by Tokio's `Instant::far_future()`
            // https://github.com/tokio-rs/tokio/blob/365269adaf6ec75743c0693f2378c3c6d04f806b/tokio/src/time/instant.rs#L57-L63
            //
            // See also https://internals.rust-lang.org/t/instant-systemtime-min-max/21375/16
            interval = 86400 * 365 * 30
        }

        self.last_io
            .checked_add(Duration::from_secs(interval))
            .unwrap()
    }

    /// Retrieve an input buffer. The event loop must fill the buffer and pass it to `Self::try_decode()`.
    pub fn get_read_buffer(&mut self) -> BytesMut {
        match self.state {
            State::StartOfHeader => {
                trace!("Waiting for start of header.");
                BytesMut::zeroed(2)
            }
            State::EndOfHeader { .. } => {
                trace!("Waiting for end of the header.");
                BytesMut::zeroed(2)
            }
            State::RestOfPacket {
                bytes_remaining, ..
            } => {
                trace!("Waiting for remainder of the packet.");
                BytesMut::zeroed(bytes_remaining as usize)
            }
        }
    }

    /// Retrieve bytes that must be transmitted to the server.
    ///
    /// `Ok(None)` indicates no bytes are ready to be sent.
    /// `Err()` indicates that the connection must be closed.
    pub fn poll_transmits(&mut self, now: Instant) -> Result<Option<Bytes>, ClientDisconnected> {
        if self.connection_status == ConnectionStatus::Disconnected {
            return Err(ClientDisconnected);
        }

        if self.connection_status == ConnectionStatus::NotConnected {
            self.connection_status = ConnectionStatus::Connecting;

            let packet: Packet = self.connect.clone().into();
            debug!("<-- {packet:?}");
            self.statistics.record_outbound_packet(&packet);

            self.last_io = now;
            return Ok(Some(packet.into_bytes()));
        }
        if self.connection_status == ConnectionStatus::Connecting {
            return Ok(None);
        }

        if let Some(packet) = self.transmits.pop() {
            if let Packet::Disconnect(..) = &packet {
                self.connection_status = ConnectionStatus::Disconnected;
            };
            self.last_io = now;
            debug!("<-- {packet:?}");
            self.statistics.record_outbound_packet(&packet);

            return Ok(Some(packet.into_bytes()));
        }

        Ok(None)
    }

    // Try parsing the bytes as a Packet.
    pub fn try_decode(&mut self, buf: Bytes, _now: Instant) -> Option<Packet> {
        let (state, packet) = match &self.state {
            State::StartOfHeader => {
                // MQTT uses between 1 and 3 (including) bytes to encode the
                // length of the packet.
                let packet_length = match decode::packet_length(&buf[1..]) {
                    Ok(packet_length) => packet_length,
                    // `buf` doesn't contain enough bytes to decode the length.
                    // At maximum, 2 more bytes are required to make the header complete.
                    Err(decode::DecodingError::NotEnoughBytes { .. }) => {
                        self.state = State::EndOfHeader {
                            partial_header: buf,
                        };
                        return None;
                    }
                    Err(error) => {
                        error!("Failed to decode packet length: {error:?}");
                        return None;
                    }
                };

                let bytes_remaining = packet_length - buf.len() as u32;
                if bytes_remaining == 0 {
                    match Packet::try_from(buf) {
                        Ok(packet) => {
                            debug!("--> {packet:?}");

                            return Some(packet);
                        }
                        Err(error) => {
                            error!("Failed to parse a 4 byte packet: {error:?}");
                            return None;
                        }
                    };
                }

                (
                    State::RestOfPacket {
                        header: buf,
                        bytes_remaining,
                    },
                    None,
                )
            }
            State::EndOfHeader { partial_header } => {
                let mut header = BytesMut::new();
                header.put(partial_header.clone());
                header.put(buf);

                let packet_length = match decode::packet_length(&header[1..]) {
                    Ok(packet_length) => packet_length,
                    Err(error) => {
                        error!("Failed to decode packet length: {error:?}");
                        return None;
                    }
                };

                let bytes_remaining = packet_length - header.len() as u32;
                (
                    State::RestOfPacket {
                        header: header.freeze(),
                        bytes_remaining,
                    },
                    None,
                )
            }

            State::RestOfPacket {
                header: prefix,
                bytes_remaining: length,
            } => {
                if buf.len() < *length as usize {
                    let remaining_length = length - buf.len() as u32;
                    let mut partial_packet = BytesMut::new();
                    partial_packet.put(prefix.clone());
                    partial_packet.put(buf);

                    self.state = State::RestOfPacket {
                        header: partial_packet.freeze(),
                        bytes_remaining: remaining_length,
                    };
                    return None;
                }

                let mut bytes = BytesMut::with_capacity(*length as usize + prefix.len());
                bytes.put(prefix.clone());
                bytes.put(buf);

                let packet = Packet::try_from(bytes.freeze()).unwrap();

                if packet.packet_type() == PacketType::ConnAck {
                    self.connection_status = ConnectionStatus::Connected;
                }
                self.statistics.record_inbound_packet(&packet);

                // parse message;
                (State::StartOfHeader, Some(packet))
            }
        };

        self.state = state;
        packet.as_ref().inspect(|ref packet| {
            debug!("--> {packet:?}");
        });
        packet
    }

    pub fn send(&mut self, packet: Packet) {
        self.transmits.push(packet);
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
enum ConnectionStatus {
    #[default]
    NotConnected,

    Connecting,
    Connected,

    // The client has terminated the connection.
    Disconnected,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientDisconnected;

#[derive(Debug, Default)]
struct Statistics {
    pub bytes_read: usize,
    pub bytes_sent: usize,
    pub packets_read: usize,
    pub packets_sent: usize,
}

impl Statistics {
    fn record_inbound_packet(&mut self, packet: &Packet) {
        self.bytes_read += packet.length();
        self.packets_read += 1;
    }

    fn record_outbound_packet(&mut self, packet: &Packet) {
        self.bytes_sent += packet.length();
        self.packets_sent += 1;
    }
}

/// Type indicating that the connection between a handle and the is broken.
/// It's likely happened after the connection to the MQTT server broke.
#[derive(Debug)]
pub struct ConnectionError;

impl Error for ConnectionError {}

impl Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The connection to the `Client` is broken. It probably happened because the connection to the MQTT server broke.")
    }
}

#[cfg(any(feature = "blocking", feature = "async"))]
impl From<async_channel::RecvError> for ConnectionError {
    fn from(_: async_channel::RecvError) -> Self {
        ConnectionError
    }
}

#[cfg(any(feature = "blocking", feature = "async"))]
impl<T> From<async_channel::SendError<T>> for ConnectionError {
    fn from(_: async_channel::SendError<T>) -> Self {
        ConnectionError
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ConnAck;
    use std::io::{Cursor, Read};

    fn as_str(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).expect("Failed to parse bytes as UTF-8.")
    }

    fn decode_message(packet: Packet) -> Packet {
        let bytes = packet.into_bytes();
        let mut binding = MqttBinding::from_connect(Connect::builder().build());
        let mut offset = 0;

        loop {
            let mut buffer = binding.get_read_buffer();
            let size = buffer.len();

            buffer.copy_from_slice(&bytes[offset..offset + size]);
            offset += size;
            if let Some(packet) = binding.try_decode(buffer.freeze(), Instant::now()) {
                return packet;
            }
        }
    }

    #[test]
    fn test_publish() {
        let packet = publish("zigbee2mqtt/light/state", Bytes::from(r#"{"state":"on"}"#));

        let packet = decode_message(packet.into());

        assert_eq!(packet.length(), 41);
        assert_eq!(packet.packet_type(), PacketType::Publish);
        // assert_eq!(packet.topic(), "$SYS/broker/uptime");
        assert_eq!(as_str(packet.payload()), r#"{"state":"on"}"#);

        let packet = publish("$SYS/broker/uptime", Bytes::from(r#"388641 seconds"#));

        let packet = decode_message(packet.into());
        assert_eq!(packet.packet_type(), PacketType::Publish);
        assert_eq!(packet.length(), 36);
        // assert_eq!(packet.topic(), "$SYS/broker/uptime");
        assert_eq!(as_str(packet.payload()), "388641 seconds");

        let packet = publish(
            "zigbee2mqtt/binary-switch",
            Bytes::from(r#"{"action":"off","battery":100,"linkquality":3,"voltage":1400}"#),
        );
        let packet = decode_message(packet.into());
        assert_eq!(packet.packet_type(), PacketType::Publish);
        // assert_eq!(packet.topic(), "zigbee2mqtt/binary-switch");
        assert_eq!(
            as_str(packet.payload()),
            "{\"action\":\"off\",\"battery\":100,\"linkquality\":3,\"voltage\":1400}"
        );

        let packet = publish(
            "zigbee2mqtt/thermo-hygrometer",
            Bytes::from(
                r#"{"battery":100,"comfort_humidity_max":60,"comfort_humidity_min":40,"comfort_temperature_max":27,"comfort_temperature_min":19,"humidity":47.2,"linkquality":105,"temperature":24,"temperature_units":"fahrenheit","update":{"installed_version":4105,"latest_version":8960,"state":"available"}}"#,
            ),
        );

        let packet = decode_message(packet.into());

        assert_eq!(packet.packet_type(), PacketType::Publish);
        // assert_eq!(packet.topic(), "zigbee2mqtt/thermo-hygrometer");
        pretty_assertions::assert_eq!(
            as_str(packet.payload()),
            "{\"battery\":100,\"comfort_humidity_max\":60,\"comfort_humidity_min\":40,\"comfort_temperature_max\":27,\"comfort_temperature_min\":19,\"humidity\":47.2,\"linkquality\":105,\"temperature\":24,\"temperature_units\":\"fahrenheit\",\"update\":{\"installed_version\":4105,\"latest_version\":8960,\"state\":\"available\"}}"
        );
    }

    /// Verify that `MqttBinding.get_read_buffer()`
    /// and `MqttBinding.try_decode()` correctly decode `Packet`s.
    ///
    /// This test iterates over a series of valid packets. Each packet
    /// is deserialized as `Bytes` and fed to `MqttBinding`. The latter
    /// should correctly decode the `Bytes` back into the `Packet` we started with.
    #[test]
    fn test_mqtt_binding_decoding_packets() {
        let mut binding = MqttBinding::from_connect(Connect::builder().build());

        for test in valid_packets() {
            let mut input = Cursor::new(test.clone().into_bytes());

            let mut iterations = 0;
            // A mini-event loop that requests one or more read buffers
            // to decode packets.
            let packet = loop {
                iterations += 1;
                let mut buffer = binding.get_read_buffer();
                _ = input.read(&mut buffer).unwrap();

                if let Some(packet) = binding.try_decode(buffer.freeze(), Instant::now()) {
                    break packet;
                }
            };

            assert!(iterations > 0);
            assert!(iterations < 4);
            assert_eq!(test.into_bytes(), packet.into_bytes());
        }
    }

    // A collection of valid `Packet`s.
    fn valid_packets() -> Vec<Packet> {
        vec![
            PingReq.into(),
            connect("test".to_string(), 300),
            Connect::builder()
                .username("admin")
                .password("secret")
                .build()
                .into(),
            ConnAck::builder().build().into(),
        ]
    }

    // Issue #53 tracks a bug where the MqttBinding enters a hot loop
    // when the keep alive interval is 0.
    //
    // This test verifies the fix for that. First, it creates a binding with
    // a keep alive interval of 5 seconds. `MqttBinding.poll_timeout()` returns
    // an Instant that's about 5 seconds in the future.
    //
    // Then, the test is repeated with a keep alive interval of 0. Now, the Instant
    // is 30 years in the future instead of 0 seconds.
    #[test]
    fn gh_53_test_fix_for_keep_alive_interval_of_0() {
        let connect = Connect::builder().keep_alive(5).build();

        let mut binding = MqttBinding::from_connect(connect);
        let interval = binding.poll_timeout() - Instant::now();
        assert_eq!(interval.as_secs_f32().round(), 5.0);

        // Now, try again with a keep alive interval of 0 seconds.
        let connect = Connect::builder().keep_alive(0).build();

        let mut binding = MqttBinding::from_connect(connect);
        let interval = binding.poll_timeout() - Instant::now();

        assert_eq!(interval.as_secs_f32().round(), 946080000.0);
    }
}
