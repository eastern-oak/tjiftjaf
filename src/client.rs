//! An asynchronous MQTT `Client`.
//!
//! The `Client` holds a connection internally. Use a `ClientHandle` to
//! read and write packets to this connection.
use super::{MqttBinding, Packet, Publish, Subscribe};
use async_channel::{self, Receiver, RecvError, SendError, Sender};
use async_io::Timer;
use bytes::Bytes;
use futures_lite::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, FutureExt};
use log::{debug, error};
use std::time::Instant;

pub struct Client<S: AsyncRead + AsyncWrite + Unpin> {
    // Socket for interacting with the MQTT broker.
    socket: S,

    binding: MqttBinding,

    // `ClientHandle`s allow the application to send `Packet`s to the broker.
    // For example, to subscribe to a topic or to publish a message.
    // This packets are send to `receiver`. The `Client` sends those
    // to the MQTT broker.
    receiver: Receiver<Packet>,
    sender: Sender<Packet>,

    broadcast: async_broadcast::Sender<Packet>,
    _inactive_receiver: async_broadcast::InactiveReceiver<Packet>,
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(options: Options, socket: S) -> Self {
        // TODO: What size do we need?
        let (tx, rx) = async_channel::bounded(100);
        // TODO: What size do we need?
        let (broadcast, receiver) = async_broadcast::broadcast(100);
        Self {
            socket,
            binding: MqttBinding::from_options(options),
            receiver: rx,
            sender: tx,
            broadcast,
            _inactive_receiver: receiver.deactivate(),
        }
    }

    /// Spawn an event loop that operates on the socket.
    pub fn spawn(
        self,
    ) -> (
        ClientHandle,
        impl Future<Output = Result<(), std::io::Error>>,
    ) {
        let handle = ClientHandle {
            sender: self.sender.clone(),
            receiver: self.broadcast.new_receiver(),
        };
        (handle, self.run())
    }

    async fn run(mut self) -> Result<(), std::io::Error> {
        // In this loop, check with the binding if any outbound
        // packets are waiting. We call them 'transmits'. Send all pending
        // transmits to the broker.
        //
        // When done, request a read buffer, read bytes from the broker until
        // the buffer is full. Then, request the binding to decode the buffer.
        // This operation might yield a mqtt::Packet for futher processing.
        loop {
            while let Ok(packet) = self.receiver.try_recv() {
                self.binding.send(packet);
            }

            while let Some(bytes) = self.binding.poll_transmits(Instant::now()) {
                self.socket.write_all(&bytes).await?;
            }

            let timeout = self.binding.poll_timeout();
            let mut buffer = self.binding.get_read_buffer();

            enum Winner {
                Future1(Result<usize, std::io::Error>),
                Future2,
                Future3(Result<Packet, RecvError>),
            }

            let future1 = async { Winner::Future1(self.socket.read(&mut buffer).await) };
            let future2 = async {
                Timer::at(timeout).await;
                Winner::Future2
            };
            let future3 = async { Winner::Future3(self.receiver.recv().await) };

            let winner = future1.race(future2).race(future3).await;
            match winner {
                Winner::Future1(Ok(bytes_read)) => {
                    if bytes_read == 0 {
                        error!("Packet empty, reconnecting!");
                        return Err(std::io::Error::other("Packet is empty"));
                    }

                    debug!(
                        "Received {bytes_read} bytes for a buffer of {}",
                        buffer.len()
                    );

                    if let Some(packet) = self
                        .binding
                        .try_decode(buffer.freeze().slice(0..bytes_read), Instant::now())
                    {
                        if self.broadcast.broadcast(packet).await.is_err() {
                            // TODO: Change error type. std::io::Error is not really fitting here.
                            return Err(std::io::Error::other("Failed to broadcast message"));
                        }
                    }
                }
                Winner::Future1(Err(error)) => {
                    return Err(error);
                }
                Winner::Future2 => {
                    self.binding.handle_timeout(Instant::now());
                }
                Winner::Future3(Ok(packet)) => self.binding.send(packet),
                Winner::Future3(Err(_)) => {
                    return Err(std::io::Error::other("Failed to read message from channel"));
                }
            }
        }
    }
}

pub struct Options {
    // A string identifying the client. Client ids are exclusive.
    // If a client id is re-used, the broker must terminate all other connections using with that client id.
    pub client_id: Option<String>,
    pub keep_alive: u16,

    pub username: Option<String>,
    pub password: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            client_id: None,
            keep_alive: 300,
            username: None,
            password: None,
        }
    }
}

#[derive(Clone)]
pub struct ClientHandle {
    // Send packets to the `Client`.
    sender: Sender<Packet>,

    // Receive packets from the `Client`
    pub receiver: async_broadcast::Receiver<Packet>,
}

impl ClientHandle {
    pub async fn subscribe(&self, topic: impl Into<String>) -> Result<(), SendError<Packet>> {
        let packet = Subscribe::builder().add_topic(topic.into()).build_packet();
        self.send(packet).await
    }

    pub async fn publish(
        &self,
        topic: impl Into<String>,
        payload: Bytes,
    ) -> Result<(), SendError<Packet>> {
        let packet = Publish::builder()
            .topic(topic.into())
            .payload(payload)
            .build_packet();
        self.send(packet).await
    }

    pub async fn send(&self, packet: Packet) -> Result<(), SendError<Packet>> {
        self.sender.send(packet).await
    }

    pub async fn any_packet(&mut self) -> Result<Packet, async_broadcast::RecvError> {
        self.receiver.recv().await
    }

    pub async fn subscriptions(&mut self) -> Result<Publish, async_broadcast::RecvError> {
        loop {
            let packet = self.any_packet().await?;
            if let Packet::Publish(publish) = packet {
                return Ok(publish);
            }
        }
    }
}
