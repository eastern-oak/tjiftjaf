//! A blocking MQTT [`Client`].
//!
//! After creating the `Client`, [`Client::spawn()`] runs the
//! client in a new thread. That method returns a [`ClientHandle`]
//! allowing an application to [subscribe](ClientHandle::subscribe()) to topics, [publish](ClientHandle::publish()) messages and [retrieve
//! publications](ClientHandle::publication()).
//!
//! Below you find a small snippet. Also, take a look at [examples/blocking_client.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/blocking_client.rs)
//! for a more complete example.
//!
//! ```no_run
//! use std::net::TcpStream;
//! use tjiftjaf::{Connect, QoS, blocking::Client, packet_identifier};
//!
//! let stream = TcpStream::connect("localhost:1883").unwrap();
//! let connect = Connect::builder()
//!   .client_id("tjiftjaf")
//!   .build();
//!
//! let client = Client::new(connect, stream);
//!
//! // Spawn the client in a new thread and obtain a handle to it.
//! let (mut handle, _task) = client.spawn().unwrap();
//!
//! // Use the handle to subscribe to topics...
//! handle.subscribe("$SYS/broker/uptime", QoS::AtMostOnceDelivery).unwrap();
//!
//! // ...to publish messages...
//! handle.publish("some-topic", r"payload".into()).unwrap();
//!
//! // ...or to wait for publications on topics you subscribed to.
//! let publication = handle.publication().unwrap();
//! println!("Received message on topic {}", publication.topic());
//! ```
use crate::{Connect, Disconnect, HandlerError, MqttBinding, Packet, Publish, QoS, Subscribe};
use async_channel::{Receiver, Sender};
use log::info;
use mio::{Events, Interest, Poll, Token, Waker};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    thread::{self, JoinHandle},
    time::Instant,
};

const CLIENT: Token = Token(0);
const PUBLISH: Token = Token(1);

/// A blocking client to interact with a MQTT broker.
///
/// See the [module documentation](crate::blocking) for more information.
pub struct Client {
    socket: mio::net::TcpStream,
    binding: MqttBinding,
}

impl Client {
    /// Create a new `Client`.
    pub fn new(connect: Connect, socket: TcpStream) -> Self {
        Self {
            socket: mio::net::TcpStream::from_std(socket),
            binding: MqttBinding::from_connect(connect),
        }
    }

    /// Start a new thread and move the `Client` to it.
    pub fn spawn(
        self,
    ) -> Result<(ClientHandle, JoinHandle<Result<(), std::io::Error>>), std::io::Error> {
        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), PUBLISH)?;

        // TODO: GH-83 decide on capacity of channel.
        // For communication _to_ the handler.
        let (to_tx, to_rx) = async_channel::bounded(100);
        // For communication _from_ the handler.
        let (from_tx, from_rx) = async_channel::bounded(100);
        let handle = ClientHandle::new(from_tx, to_rx, waker);

        Ok((
            handle,
            thread::spawn(move || self.run(poll, to_tx, from_rx)),
        ))
    }

    fn run(
        mut self,
        mut poll: Poll,
        sender: Sender<Packet>,
        receiver: Receiver<Packet>,
    ) -> Result<(), std::io::Error> {
        let mut events = Events::with_capacity(128);
        poll.registry()
            .register(&mut self.socket, CLIENT, Interest::READABLE)?;

        // In this loop, check with the binding if any outbound
        // packets are waiting. We call them 'transmits'. Send all pending
        // transmits to the broker.
        //
        // When done, request a read buffer, read bytes from the broker until
        // the buffer is full. Then, request the binding to decode the buffer.
        // This operation might yield a mqtt::Packet for further processing.
        loop {
            while let Ok(packet) = receiver.try_recv() {
                self.binding.send(packet);
            }

            loop {
                match self.binding.poll_transmits(Instant::now()) {
                    Ok(Some(bytes)) => {
                        self.socket.write_all(&bytes)?;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        self.socket.shutdown(Shutdown::Both)?;
                        info!("The client disconnected")
                    }
                }
            }

            let timeout = self.binding.poll_timeout();
            poll.poll(&mut events, Some(timeout - Instant::now()))?;

            for event in events.iter() {
                if event.token() == PUBLISH {
                    while let Ok(packet) = receiver.try_recv() {
                        self.binding.send(packet);
                    }
                }

                if event.token() != CLIENT {
                    continue;
                }

                if !event.is_readable() {
                    continue;
                }

                loop {
                    let mut buffer = self.binding.get_read_buffer();
                    self.socket.read_exact(&mut buffer)?;

                    // TODO: If packet is invalid, try_decode() never returns a `Some`,
                    // And thus the `loop` never breaks.
                    // Maybe `try_decode` should return an Error. Maybe with variant `NotEnoughBytes`
                    // to indicate that more bytes are expected and event loop should continue.
                    // Any other error indicates an issue and event loop must break the loop
                    if let Some(packet) = self.binding.try_decode(buffer.freeze(), Instant::now()) {
                        sender
                            .send_blocking(packet)
                            .map_err(std::io::Error::other)?;
                        break;
                    };
                }
            }
        }
    }
}

/// A handle to interact with a [`Client`].
///
/// See the [module documentation](crate::blocking) for more information.
pub struct ClientHandle {
    // Send packets to the `Client`.
    sender: Sender<Packet>,

    // Receive packets from the `Client`
    receiver: Receiver<Packet>,

    waker: Waker,
}

impl ClientHandle {
    fn new(sender: Sender<Packet>, receiver: Receiver<Packet>, waker: Waker) -> Self {
        Self {
            sender,
            receiver,
            waker,
        }
    }

    /// Subscribe to the given `topic` using the provided `qos`.
    ///
    /// ```no_run
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{Connect, blocking::Client, packet_identifier, QoS};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    /// handle.subscribe("sensor/temperature/1", QoS::AtMostOnceDelivery).unwrap();
    /// while let Ok(publish) = handle.publication() {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// ```
    pub fn subscribe(&self, topic: impl Into<String>, qos: QoS) -> Result<(), HandlerError> {
        let packet = Subscribe::builder(topic, qos).build_packet();
        self.send(packet)
    }

    /// Publish `payload` to the given `topic`.
    ///
    /// ```no_run
    /// use bytes::Bytes;
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{Connect, blocking::Client, packet_identifier};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    /// handle
    ///     .publish("sensor/temperature/1", Bytes::from("26.1"))
    ///     .unwrap();
    ///```
    pub fn publish(
        &self,
        topic: impl Into<String>,
        payload: bytes::Bytes,
    ) -> Result<(), HandlerError> {
        let packet = Publish::builder(topic, payload).build_packet();
        self.send(packet)
    }

    /// Send any `Packet` to the broker.
    fn send(&self, packet: Packet) -> Result<(), HandlerError> {
        self.sender.send_blocking(packet)?;
        self.waker.wake().map_err(|error| {
            HandlerError(format!(
                "Failed sending packet: couldn't wake the waker: {error:?}"
            ))
        })?;
        Ok(())
    }

    fn any_packet(&mut self) -> Result<Packet, HandlerError> {
        let packet = self.receiver.recv_blocking()?;
        Ok(packet)
    }

    /// Wait for the next [`Publish`] messages emitted by the broker.
    ///
    /// ```no_run
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{Connect, blocking::Client, packet_identifier, QoS};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    /// handle.subscribe("sensor/temperature/1", QoS::AtMostOnceDelivery).unwrap();
    /// while let Ok(publish) = handle.publication() {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// ```
    pub fn publication(&mut self) -> Result<Publish, HandlerError> {
        loop {
            let packet = self.any_packet()?;
            if let Packet::Publish(publish) = packet {
                return Ok(publish);
            }
        }
    }

    /// Emit a [`Disconnect`] to terminate the connection.
    pub fn disconnect(self) -> Result<(), HandlerError> {
        self.send(Disconnect.into())
    }
}
