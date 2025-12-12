//! An asynchronous MQTT [`Client`].
//!
//! After creating the `Client`, [`Client::spawn()`] runs the
//! client in a new future. That method returns a [`ClientHandle`] and a future.
//! The application is responsible for `await`ing the future and running the client.
//!
//! The `ClientHandle` can be used to [subscribe](crate::subscribe()) to topics, [publish](crate::publish()) messages and [retrieve
//! publications](ClientHandle::subscriptions()).
//!
//! Below you find a small snippet based on the executor smol. Also, take a look at [examples/client_with_smol.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/client_with_smol.rs)
//! and [examples/client_with_tokio.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/client_with_tokio.rs)
//!
//! ```no_run
//! use async_net::TcpStream;
//! use futures_lite::FutureExt;
//! use tjiftjaf::{publish, subscribe, Connect, QoS, aio::{Client, Emit}, packet_identifier};
//!
//! smol::block_on(async {
//!   let stream = TcpStream::connect("localhost:1883").await.unwrap();
//!   let connect = Connect::builder()
//!     .client_id("tjiftjaf")
//!     .build();
//!
//!   let client = Client::new(connect, stream);
//!
//!   // Move the client in a future and obtain a handle to it.
//!   let (mut handle, task) = client.spawn();
//!
//!   task.race(async {
//!     // Use the handle to subscribe to topics...
//!     subscribe("$SYS/broker/uptime").emit(&handle).await.unwrap();
//!
//!     // ...to publish messages...
//!     publish("some-topic", r"payload".into()).emit(&handle).await.unwrap();
//!
//!     // ...or to wait for publications on topics you subscribed to.
//!     let publication = handle.subscriptions().await.unwrap();
//!     println!("Received message on topic {}", publication.topic());
//!     Ok(())
//!   }).await;
//! });
//! ```
use crate::{
    Connect, ConnectionError, Disconnect, MqttBinding, Packet, PubAck, PubComp, PubRec, PubRel,
    Publish, QoS, Token,
};
use async_channel::{self, Receiver, RecvError, SendError, Sender};
use async_io::Timer;
use futures_lite::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, FutureExt};
use log::{debug, error, info, trace};
use std::{collections::HashMap, time::Instant};

#[cfg(feature = "experimental")]
pub mod server;

/// An asynchronous client to interact with a MQTT broker.
///
/// See the [module documentation](crate::aio) for more information.
pub struct Client<S: AsyncRead + AsyncWrite + Unpin> {
    // Socket for interacting with the MQTT broker.
    socket: S,
    binding: MqttBinding,

    acks: HashMap<Token, async_channel::Sender<Packet>>,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(connect: Connect, socket: S) -> Self {
        Self {
            socket,
            binding: MqttBinding::from_connect(connect),
            acks: HashMap::new(),
        }
    }

    /// Spawn an event loop that operates on the socket.
    pub fn spawn(
        self,
    ) -> (
        ClientHandle,
        impl std::future::Future<Output = Result<(), std::io::Error>>,
    ) {
        // TODO: GH-83 decide on capacity of channel.
        // For communication _to_ the handler.
        let (to_tx, to_rx) = async_channel::bounded(100);
        // For communication _from_ the handler.
        let (from_tx, from_rx) = async_channel::bounded(100);

        let handle = ClientHandle {
            sender: from_tx,
            receiver: to_rx,
        };
        (handle, self.run(to_tx, from_rx))
    }

    async fn run(
        mut self,
        sender: Sender<Packet>,
        receiver: Receiver<(Packet, Sender<Packet>)>,
    ) -> Result<(), std::io::Error> {
        // In this loop, check with the binding if any outbound
        // packets are waiting. We call them 'transmits'. Send all pending
        // transmits to the broker.
        //
        // When done, request a read buffer, read bytes from the broker until
        // the buffer is full. Then, request the binding to decode the buffer.
        // This operation might yield a mqtt::Packet for further processing.
        loop {
            while let Ok((packet, channel)) = receiver.try_recv() {
                if let Some(token) = self.binding.send(packet) {
                    self.acks.insert(token, channel);
                }
            }

            loop {
                match self.binding.poll_transmits(Instant::now()) {
                    Ok(Some(bytes)) => {
                        self.socket.write_all(&bytes).await?;
                        // If the socket implementation is buffered, `bytes` will not be transmitted unless
                        // the internal buffer is full or a call to flush is done.
                        self.socket.flush().await?;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        self.socket.close().await?;
                        info!("The client disconnected.");
                        return Ok(());
                    }
                }
            }

            let timeout = self.binding.poll_timeout();
            let mut buffer = self.binding.get_read_buffer();

            enum Winner {
                Future1(Result<usize, std::io::Error>),
                Future2,
                Future3(Result<(Packet, Sender<Packet>), RecvError>),
            }

            let future1 = async { Winner::Future1(self.socket.read(&mut buffer).await) };
            let future2 = async {
                Timer::at(timeout).await;
                Winner::Future2
            };
            let future3 = async { Winner::Future3(receiver.recv().await) };

            let winner = future1.race(future2).race(future3).await;
            match winner {
                Winner::Future1(Ok(bytes_read)) => {
                    if bytes_read == 0 {
                        error!("Packet empty, reconnecting!");
                        return Err(std::io::Error::other("Packet is empty"));
                    }

                    trace!(
                        "Received {bytes_read} bytes for a buffer of {}",
                        buffer.len()
                    );

                    if let Some((packet, token)) = self
                        .binding
                        .try_decode(buffer.freeze().slice(0..bytes_read), Instant::now())
                    {
                        if let Packet::Publish(publish) = &packet {
                            match (publish.qos(), publish.packet_identifier()) {
                                (QoS::AtMostOnceDelivery, _) => {}
                                (QoS::AtLeastOnceDelivery, Some(packet_identifier)) => {
                                    self.binding.send(PubAck::new(packet_identifier).into());
                                }
                                (QoS::ExactlyOnceDelivery, Some(packet_identifier)) => {
                                    self.binding.send(PubRec::new(packet_identifier).into());
                                }
                                (qos, maybe_packet_identifier) => {
                                    panic!(
                                        "Somehow this PUBLISH packet has {qos:?} and {maybe_packet_identifier:?}. That combination is not allowed and the tjiftjaf crate must not allow to create such packet. Please report a bug to https://github.com/eastern-oak/tjiftjaf/issues. {packet:?} "
                                    )
                                }
                            }
                        };

                        if let Packet::PubRec(packet) = &packet {
                            self.binding
                                .send(PubRel::new(packet.packet_identifier()).into());
                        }

                        if let Packet::PubRel(packet) = &packet {
                            self.binding
                                .send(PubComp::new(packet.packet_identifier()).into());
                        }

                        if let Some(token) = token {
                            if let Some(channel) = self.acks.remove(&token) {
                                // Don't worry if the  channel is closed. Then the receiver
                                // is not caring about the confirmation.
                                let _ = channel.send(packet).await;
                            }

                            continue;
                        }

                        if sender.send(packet).await.is_err() {
                            // TODO: Change error type. std::io::Error is not really fitting here.
                            return Err(std::io::Error::other("Failed to send message to handler"));
                        }
                    }
                }
                Winner::Future1(Err(error)) => {
                    return Err(error);
                }
                Winner::Future2 => {
                    self.binding.handle_timeout(Instant::now());
                }
                Winner::Future3(Ok((packet, channel))) => {
                    let token = self.binding.send(packet);
                    if let Some(token) = token {
                        debug!("Insert token {token:?}");
                        self.acks.insert(token, channel);
                    }
                }
                Winner::Future3(Err(_)) => {
                    return Err(std::io::Error::other("Failed to read message from channel"));
                }
            }
        }
    }
}

/// A handle to interact with a [`Client`].
///
/// See the [module documentation](crate::aio) for more information.
pub struct ClientHandle {
    // Send packets to the `Client`.
    sender: Sender<(Packet, Sender<Packet>)>,

    // Receive packets from the `Client`
    receiver: Receiver<Packet>,
}

impl ClientHandle {
    pub(crate) async fn send(
        &self,
        packet: Packet,
    ) -> Result<Receiver<Packet>, SendError<(Packet, Sender<Packet>)>> {
        let (tx, rx) = async_channel::bounded(1);

        self.sender.send((packet, tx)).await.map(|_| rx)
    }

    /// Wait for the next [`Publish`] messages emitted by the broker.
    ///
    /// ```no_run
    /// # use async_net::TcpStream;
    /// # use futures_lite::FutureExt;
    /// # use tjiftjaf::{subscribe, Connect, QoS, aio::{Emit, Client}, packet_identifier};
    /// # smol::block_on(async {
    /// # let stream = TcpStream::connect("localhost:1883").await.unwrap();
    /// # let connect = Connect::builder().build();
    /// # let client = Client::new(connect, stream);
    /// # let (mut handle, task) = client.spawn();
    /// subscribe("sensor/temperature/1").emit(&handle).await.unwrap();
    /// while let Ok(publish) = handle.subscriptions().await {
    ///    println!(
    ///       "On topic {} received {:?}",
    ///        publish.topic(),
    ///        publish.payload()
    ///   );
    /// }
    /// # });
    /// ```
    pub async fn subscriptions(&mut self) -> Result<Publish, ConnectionError> {
        loop {
            let packet = self.receiver.recv().await?;
            if let Packet::Publish(publish) = packet {
                return Ok(publish);
            }
        }
    }

    /// Emit a [`Disconnect`] to terminate the connection.
    pub async fn disconnect(self) -> Result<(), ConnectionError> {
        self.send(Disconnect.into()).await?;
        Ok(())
    }
}

// A trait for sending messages via [`ClientHandle`] to a server.
pub trait Emit {
    /// Send a message to a client.
    fn emit(
        self,
        handler: &ClientHandle,
    ) -> impl std::future::Future<Output = Result<Receiver<Packet>, ConnectionError>>;
}
