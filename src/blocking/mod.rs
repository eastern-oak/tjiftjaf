//! A blocking MQTT [`Client`].
//!
//! After creating the `Client`, [`Client::run()`] starts the
//! client. One can interact with it using [`ClientHandle`].
//!
//! `run()` blocks the calling thread until the connection breaks, so it's
//! typically driven from its own [`std::thread`]. If the connection breaks,
//! `run()` returns and the caller can reconnect by calling `run()` again
//! with a new socket; the `Client` keeps its internal state (e.g. pending
//! subscriptions) across reconnects. `run()` accepts anything `mio` can
//! register for readiness notifications; for a plain
//! [`std::net::TcpStream`], use [`Client::run_from_std()`](Client::run_from_std) instead.
//!
//! The `ClientHandle` allows an application to [subscribe](crate::Subscribe::emit()) to topics, [publish](crate::Publish::emit()) messages and [retrieve
//! publications](ClientHandle::publication()).
//!
//! Below you find a small snippet. Also, take a look at [examples/blocking_client.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/blocking_client.rs)
//! for a more complete example.
//!
//! ```no_run
//! use std::net::TcpStream;
//! use std::thread;
//! use tjiftjaf::{publish, subscribe, Connect, blocking::{Client, Emit}, packet_identifier};
//!
//! let stream = TcpStream::connect("localhost:1883").unwrap();
//! let connect = Connect::builder()
//!   .client_id("tjiftjaf")
//!   .build()
//!   .unwrap();
//!
//! let (mut client, mut handle) = Client::new(connect).unwrap();
//!
//! // Run the client on its own thread. `run_from_std()` returns when the
//! // connection breaks, so the thread can reconnect by calling it again.
//! let task = thread::spawn(move || client.run_from_std(stream));
//!
//! // Use the handle to subscribe to topics...
//! subscribe("$SYS/broker/uptime")
//!    .unwrap()
//!    .emit(&handle)
//!    .unwrap();
//!
//! // ...to publish messages...
//! publish("some-topic", r"payload")
//!    .unwrap()
//!    .emit(&handle)
//!    .unwrap();
//!
//! // ...or to wait for publications on topics you subscribed to.
//! let publication = handle.publication().unwrap();
//! println!("Received message on topic {}", publication.topic());
//! ```
use crate::{Connect, ConnectionError, Disconnect, MqttBinding, Packet, Publish};
use async_channel::{Receiver, Sender};
use log::info;
use mio::{event::Source, Events, Interest, Poll, Token, Waker};
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Instant,
};

const CLIENT: Token = Token(0);
const PUBLISH: Token = Token(1);

/// A blocking client to interact with a MQTT broker.
///
/// See the [module documentation](crate::blocking) for more information.
pub struct Client {
    poll: Poll,
    binding: MqttBinding,

    inbox: Receiver<Packet>,
    sender: Sender<Packet>,
}

impl Client {
    /// Create a new `Client` and a [`ClientHandle`] to interact with it.
    pub fn new(connect: Connect) -> Result<(Self, ClientHandle), std::io::Error> {
        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), PUBLISH)?;

        // TODO: GH-83 decide on capacity of channel.
        // For communication _to_ the handler.
        let (to_tx, to_rx) = async_channel::bounded(100);
        // For communication _from_ the handler.
        let (from_tx, from_rx) = async_channel::bounded(100);
        let handle = ClientHandle::new(to_tx, from_rx, waker);

        let this = Self {
            poll,
            inbox: to_rx,
            sender: from_tx,
            binding: MqttBinding::from_connect(connect),
        };

        Ok((this, handle))
    }

    /// Run the client using a plain [`std::net::TcpStream`]. A convenience
    /// wrapper around [`Client::run()`] for the common case, since
    /// `std::net::TcpStream` itself doesn't implement [`mio::event::Source`].
    ///
    /// Blocks the calling thread until the connection breaks, then returns.
    /// After it returns, the `Client` can be reconnected by calling
    /// `run_from_std()` (or `run()`) again with a new socket.
    pub fn run_from_std(&mut self, socket: TcpStream) -> Result<(), std::io::Error> {
        self.run(mio::net::TcpStream::from_std(socket))
    }

    /// Run the client on the given socket. Blocks the calling thread until
    /// the connection breaks, then returns.
    ///
    /// `socket` can be any type `mio` can register for readiness
    /// notifications - e.g. [`mio::net::TcpStream`] or
    /// [`mio::net::UnixStream`]. For a plain [`std::net::TcpStream`], use
    /// [`Client::run_from_std()`] instead.
    ///
    /// After it returns, the `Client` can be reconnected by calling `run()`
    /// again with a new socket.
    pub fn run<S>(&mut self, mut socket: S) -> Result<(), std::io::Error>
    where
        S: Read + Write + Source,
    {
        let mut events = Events::with_capacity(128);
        self.poll
            .registry()
            .register(&mut socket, CLIENT, Interest::READABLE)?;

        let result = self.run_with_socket(&mut socket, &mut events);

        let _ = self.poll.registry().deregister(&mut socket);

        result
    }

    // In this loop, check with the binding if any outbound
    // packets are waiting. We call them 'transmits'. Send all pending
    // transmits to the broker.
    //
    // When done, request a read buffer, read bytes from the broker until
    // the buffer is full. Then, request the binding to decode the buffer.
    // This operation might yield a mqtt::Packet for further processing.
    fn run_with_socket<S>(
        &mut self,
        socket: &mut S,
        events: &mut Events,
    ) -> Result<(), std::io::Error>
    where
        S: Read + Write,
    {
        loop {
            while let Ok(packet) = self.inbox.try_recv() {
                self.binding.send(packet);
            }

            loop {
                match self.binding.poll_transmits(Instant::now()) {
                    Ok(Some(bytes)) => {
                        if let Err(error) = socket.write_all(&bytes) {
                            // The packet couldn't be send over the wire.
                            // It is fed back to the `MqttBinding`. After reconnecting,
                            // the packet is resend using the new connection.
                            //
                            // Without retransmitting the packet, it would be lost.
                            self.binding.connection_lost(Some(bytes));
                            return Err(error);
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        self.binding.connection_lost(None);
                        info!("The client disconnected.");
                        return Ok(());
                    }
                }
            }

            let timeout = self.binding.poll_timeout();
            if let Err(error) = self.poll.poll(events, Some(timeout - Instant::now())) {
                self.binding.connection_lost(None);
                return Err(error);
            }

            for event in events.iter() {
                if event.token() == PUBLISH {
                    while let Ok(packet) = self.inbox.try_recv() {
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
                    if let Err(error) = socket.read_exact(&mut buffer) {
                        self.binding.connection_lost(None);
                        return Err(error);
                    }

                    // TODO: If packet is invalid, try_decode() never returns a `Some`,
                    // And thus the `loop` never breaks.
                    // Maybe `try_decode` should return an Error. Maybe with variant `NotEnoughBytes`
                    // to indicate that more bytes are expected and event loop should continue.
                    // Any other error indicates an issue and event loop must break the loop
                    if let Some(packet) = self.binding.try_decode(buffer, Instant::now()) {
                        if let Err(error) = self.sender.send_blocking(packet) {
                            self.binding.connection_lost(None);
                            return Err(std::io::Error::other(error));
                        }
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
    /// # use tjiftjaf::{subscribe, Connect, blocking::{Client, Emit}, packet_identifier};
    /// # let connect = Connect::builder().build().unwrap();
    /// # let (mut client, mut handle) = Client::new(connect).unwrap();
    /// subscribe("sensor/temperature/1")
    ///     .unwrap()
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
