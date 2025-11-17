//! An asynchronous MQTT `Client`.
//!
//! The `Client` holds a connection internally. Use a `ClientHandle` to
//! read and write packets to this connection.
use crate::{
    Connect, HandlerError, MqttBinding, Packet, PubAck, PubComp, PubRec, PubRel, Publish, QoS,
    Subscribe,
};
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
}

impl<S> Client<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(connect: Connect, socket: S) -> Self {
        Self {
            socket,
            binding: MqttBinding::from_connect(connect),
        }
    }

    /// Spawn an event loop that operates on the socket.
    pub fn spawn(
        self,
    ) -> (
        ClientHandle,
        impl Future<Output = Result<(), std::io::Error>>,
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
        receiver: Receiver<Packet>,
    ) -> Result<(), std::io::Error> {
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

            while let Some(bytes) = self.binding.poll_transmits(Instant::now()) {
                self.socket.write_all(&bytes).await?;
                // If the socket implementation is buffered, `bytes` will not be transmitted unless
                // the internal buffer is full or a call to flush is done.
                self.socket.flush().await?;
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
            let future3 = async { Winner::Future3(receiver.recv().await) };

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
                Winner::Future3(Ok(packet)) => self.binding.send(packet),
                Winner::Future3(Err(_)) => {
                    return Err(std::io::Error::other("Failed to read message from channel"));
                }
            }
        }
    }
}

pub struct ClientHandle {
    // Send packets to the `Client`.
    sender: Sender<Packet>,

    // Receive packets from the `Client`
    receiver: Receiver<Packet>,
}

impl ClientHandle {
    pub async fn subscribe(
        &self,
        topic: impl Into<String>,
        qos: QoS,
    ) -> Result<(), SendError<Packet>> {
        let packet = Subscribe::builder(topic, qos).build_packet();
        self.send(packet).await
    }

    pub async fn publish(
        &self,
        topic: impl Into<String>,
        payload: Bytes,
    ) -> Result<(), SendError<Packet>> {
        let packet = Publish::builder(topic, payload).build_packet();
        self.send(packet).await
    }

    pub async fn send(&self, packet: Packet) -> Result<(), SendError<Packet>> {
        self.sender.send(packet).await
    }

    pub async fn any_packet(&mut self) -> Result<Packet, HandlerError> {
        let packet = self.receiver.recv().await?;
        Ok(packet)
    }

    pub async fn subscriptions(&mut self) -> Result<Publish, HandlerError> {
        loop {
            let packet = self.any_packet().await?;
            if let Packet::Publish(publish) = packet {
                return Ok(publish);
            }
        }
    }
}
