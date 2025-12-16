//! A blocking MQTT [`Client`].
//!
//! After creating the `Client`, [`Client::spawn()`] runs the
//! client in a new thread. That method returns a [`ClientHandle`]
//! allowing an application to [subscribe](crate::Subscribe::emit()) to topics, [publish](crate::Publish::emit()) messages and [retrieve
//! publications](ClientHandle::publication()).
//!
//! Below you find a small snippet. Also, take a look at [examples/blocking_client.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/blocking_client.rs)
//! for a more complete example.
//!
//! ```no_run
//! use std::net::TcpStream;
//! use tjiftjaf::{publish, subscribe, Connect, blocking::{Client, Emit}, packet_identifier};
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
//! subscribe("$SYS/broker/uptime")
//!    .emit(&handle)
//!    .unwrap();
//!
//! // ...to publish messages...
//! publish("some-topic", r"payload".into())
//!    .emit(&handle)
//!    .unwrap();
//!
//! // ...or to wait for publications on topics you subscribed to.
//! let publication = handle.publication().unwrap();
//! println!("Received message on topic {}", publication.topic());
//! ```
use crate::{Connect, ConnectionError, Disconnect, MqttBinding, Packet, Publish};
use async_channel::{Receiver, Sender};
use bytes::Bytes;
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
                        info!("The client disconnected.");
                        return Ok(());
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
                    if let Some(packet) = self
                        .binding
                        .try_decode(Bytes::copy_from_slice(&buffer), Instant::now())
                    {
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

    /// Send any `Packet` to the broker.
    pub(crate) fn send(&self, packet: Packet) -> Result<(), ConnectionError> {
        self.sender.send_blocking(packet)?;
        self.waker.wake().map_err(|_| ConnectionError)?;
        Ok(())
    }

    /// Wait for the next [`Publish`] messages emitted by the broker.
    ///
    /// ```no_run
    /// # use std::net::TcpStream;
    /// # use tjiftjaf::{subscribe, Connect, blocking::{Client, Emit}, packet_identifier};
    /// # let stream = TcpStream::connect("localhost:1883").unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, _task) = client.spawn().unwrap();
    /// subscribe("sensor/temperature/1")
    ///     .emit(&handle)
    ///     .unwrap();
    /// while let Ok(publish) = handle.publication() {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// ```
    pub fn publication(&mut self) -> Result<Publish, ConnectionError> {
        loop {
            let packet = self.receiver.recv_blocking()?;
            if let Packet::Publish(publish) = packet {
                return Ok(publish);
            }
        }
    }

    /// Emit a [`Disconnect`] to terminate the connection.
    pub fn disconnect(&self) -> Result<(), ConnectionError> {
        self.send(Disconnect.into())
    }
}

/// A trait for sending messages via [`ClientHandle`] to a server.
pub trait Emit {
    /// Send a message via the the client to the broker.
    fn emit(self, handler: &ClientHandle) -> Result<(), ConnectionError>;
}
