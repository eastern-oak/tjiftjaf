//! The `tjiftjaf` crate provides a asynchronous MQTT [`Client`].
//!
//! # Example
//!
//! ```no_run
//! use async_net::TcpStream;
//! use tjiftjaf::{Frame, Client, Options};
//!
//! fn main() {
//!    smol::block_on(async {
//!        let stream = TcpStream::connect("test.mosquitto.org:1883")
//!            .await
//!            .expect("Failed connecting to MQTT broker.");
//!
//!        let options = Options {
//!            client_id: None,
//!            keep_alive: 300,
//!            ..Options::default()
//!        };
//!        let client = Client::new(options, stream);
//!
//!        // Spawn the event loop that monitors the socket.
//!        // `handle` allows for sending and receiving MQTT packets.
//!        let (mut handle, _task) = client.spawn();
//!
//!        handle
//!            .subscribe("$SYS/broker/uptime")
//!            .await
//!            .expect("Failed to subscribe to topic.");
//!
//!        loop {
//!            let packet = handle
//!                .subscriptions()
//!                .await
//!                .expect("Failed to read packet.");
//!
//!            let payload = String::from_utf8_lossy(packet.payload());
//!            println!("{} - {:?}", packet.topic(), payload);
//!        }
//!    })
//! }
//! ```
mod client;
mod decode;
mod encode;
pub mod packet;
pub mod packet_v2;
mod validate;
pub use client::{Client, ClientHandle, Options};
pub use packet::*;

use bytes::{BufMut, Bytes, BytesMut};
use log::{debug, error, info};
use packet_v2::{connect::Connect, ping_req::PingReq};
use std::time::{Duration, Instant, SystemTime};

pub fn packet_identifier() -> u16 {
    let seconds = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_millis(),
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

pub fn subscribe(topic: &str) -> Packet {
    Subscribe::builder()
        .add_topic(topic.to_owned())
        .build_packet()
}

pub fn publish(topic: &str, payload: Bytes) -> Packet {
    Publish::builder()
        .topic(topic.to_owned())
        .payload(payload)
        .build_packet()
}

#[derive(Default, Debug)]
enum State {
    // The state machine is waiting for the start of a new packet.
    #[default]
    AwaitingStartOfHeader,

    // The state machine processed first half of a header and is waiting
    // for the remainder of the header.
    AwaitingEndOfHeader {
        partial_header: Bytes,
    },

    // The state machine processed the header and it knows the length
    // of the entire packet. Now it waits for the remaining bytes
    // to complete the packet.
    AwaitingRestOfPacket {
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

    options: Options,
}

// The driver must do 2 things:
// * request a buffer, it'll need to read bytes from the socket and fill the buffer until it's fill.
// * request a buffer to write,
impl MqttBinding {
    pub fn from_options(options: Options) -> Self {
        Self {
            options,
            connection_status: ConnectionStatus::default(),
            state: State::default(),
            transmits: vec![],
            statistics: Statistics::default(),
            last_io: Instant::now(),
        }
    }

    pub fn handle_timeout(&mut self, now: Instant) {
        if (now - self.last_io).as_secs() >= self.options.keep_alive as u64 {
            self.transmits.push(Packet::PingReq(PingReq))
        }
    }

    pub fn poll_timeout(&mut self) -> Instant {
        self.last_io + Duration::from_secs(self.options.keep_alive as u64)
    }

    /// Retrieve an input buffer. The event loop must fill the buffer.
    pub fn get_read_buffer(&mut self) -> BytesMut {
        match self.state {
            State::AwaitingStartOfHeader => {
                debug!("Waiting for start of header.");
                BytesMut::zeroed(2)
            }
            State::AwaitingEndOfHeader { .. } => {
                debug!("Waiting for end of the header.");
                BytesMut::zeroed(2)
            }
            State::AwaitingRestOfPacket {
                bytes_remaining, ..
            } => {
                debug!("Waiting for remainder of the packet.");
                BytesMut::zeroed(bytes_remaining as usize)
            }
        }
    }

    pub fn poll_transmits(&mut self, now: Instant) -> Option<Bytes> {
        if self.connection_status == ConnectionStatus::NotConnected {
            self.connection_status = ConnectionStatus::Connecting;

            let mut builder = Connect::builder().keep_alive(self.options.keep_alive);
            if let Some(client_id) = self.options.client_id.clone() {
                builder = builder.client_id(client_id);
            }

            if let Some(username) = self.options.username.clone() {
                builder = builder.username(username);
            }

            if let Some(password) = self.options.password.clone() {
                builder = builder.password(password);
            }

            let packet = builder.build_packet();

            debug!("<-- {:?}", packet);
            self.statistics.record_outbound_packet(&packet);

            self.last_io = now;
            return Some(packet.into_bytes());
        }
        if self.connection_status == ConnectionStatus::Connecting {
            return None;
        }

        if let Some(packet) = self.transmits.pop() {
            self.last_io = now;
            debug!("<-- {:?}", packet);
            self.statistics.record_outbound_packet(&packet);

            return Some(packet.into_bytes());
        }

        None
    }

    // Try parsing the bytes as a Packet.
    pub fn try_decode(&mut self, buf: Bytes, now: Instant) -> Option<Packet> {
        let (state, packet) = match &self.state {
            State::AwaitingStartOfHeader => {
                // MQTT uses between 1 and 3 (including) bytes to encode the
                // length of the packet.
                let packet_length = match decode::packet_length(&buf[1..]) {
                    Ok(packet_length) => packet_length,
                    // `buf` doesn't contain enough bytes to decode the length.
                    // At maximum, 2 more bytes are required to make the header complete.
                    Err(decode::DecodingError::NotEnoughBytes { .. }) => {
                        self.state = State::AwaitingEndOfHeader {
                            partial_header: buf,
                        };
                        return None;
                    }
                    Err(error) => {
                        error!("Failed to decode packet length: {:?}", error);
                        return None;
                    }
                };

                let bytes_remaining = packet_length - buf.len() as u32;
                if bytes_remaining == 0 {
                    match Packet::try_from(buf) {
                        Ok(packet) => {
                            return Some(packet);
                        }
                        Err(error) => {
                            error!("Failed to parse a 4 byte packet: {}", error);
                            return None;
                        }
                    };
                }

                (
                    State::AwaitingRestOfPacket {
                        header: buf,
                        bytes_remaining,
                    },
                    None,
                )
            }
            State::AwaitingEndOfHeader { partial_header } => {
                let mut header = BytesMut::new();
                header.put(partial_header.clone());
                header.put(buf);

                let packet_length = match decode::packet_length(&header[1..]) {
                    Ok(packet_length) => packet_length,
                    Err(error) => {
                        error!("Failed to decode packet length: {:?}", error);
                        return None;
                    }
                };

                let bytes_remaining = packet_length - header.len() as u32;
                (
                    State::AwaitingRestOfPacket {
                        header: header.freeze(),
                        bytes_remaining,
                    },
                    None,
                )
            }

            State::AwaitingRestOfPacket {
                header: prefix,
                bytes_remaining: length,
            } => {
                let mut bytes = BytesMut::with_capacity(*length as usize + prefix.len());
                bytes.put(prefix.clone());
                bytes.put(buf);

                let packet = Packet::try_from(bytes.freeze()).unwrap();

                if packet.packet_type() == PacketType::ConnAck {
                    self.connection_status = ConnectionStatus::Connected;
                }
                self.statistics.record_inbound_packet(&packet);

                // parse message;
                (State::AwaitingStartOfHeader, Some(packet))
            }
        };

        self.state = state;
        packet.as_ref().inspect(|ref packet| {
            debug!("--> {:?}", packet);
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
}

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

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Cursor, Read};

    fn as_str(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).expect("Failed to parse bytes as UTF-8.")
    }

    fn decode_message(packet: Packet) -> Packet {
        let bytes = packet.into_bytes();
        let mut binding = MqttBinding::from_options(Options::default());
        let mut offset = 0;

        loop {
            let mut buffer = binding.get_read_buffer();
            let size = buffer.len();

            buffer.copy_from_slice(&bytes[offset..offset + size]);
            offset = offset + size;
            if let Some(packet) = binding.try_decode(buffer.freeze(), Instant::now()) {
                return packet;
            }
        }
    }

    #[test]
    fn test_publish() {
        let packet = publish(
            "zigbee2mqtt/light/state".into(),
            Bytes::from(r#"{"state":"on"}"#),
        );

        let packet = decode_message(packet);

        assert_eq!(packet.length(), 41);
        assert_eq!(packet.packet_type(), PacketType::Publish);
        // assert_eq!(packet.topic(), "$SYS/broker/uptime");
        assert_eq!(as_str(packet.payload()), r#"{"state":"on"}"#);

        let packet = publish(
            "$SYS/broker/uptime".into(),
            Bytes::from(r#"388641 seconds"#),
        );

        let packet = decode_message(packet);
        assert_eq!(packet.packet_type(), PacketType::Publish);
        assert_eq!(packet.length(), 36);
        // assert_eq!(packet.topic(), "$SYS/broker/uptime");
        assert_eq!(as_str(packet.payload()), "388641 seconds");

        let packet = publish(
            "zigbee2mqtt/binary-switch".into(),
            Bytes::from(r#"{"action":"off","battery":100,"linkquality":3,"voltage":1400}"#),
        );
        let packet = decode_message(packet);
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

        let packet = decode_message(packet);

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
        let mut binding = MqttBinding::from_options(Options::default());

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
        ]
    }
}
