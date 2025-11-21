use crate::{
    ConnAck, DecodingError, Frame, Packet, PingResp, SubAck,
    packet::{self, connack::ReturnCode},
};
use async_channel::{SendError, Sender};
use async_net::{TcpListener, TcpStream};
use bytes::{BufMut, BytesMut};
use futures::FutureExt;
use futures::{
    AsyncRead,
    io::{AsyncReadExt, AsyncWriteExt},
    stream::{FuturesOrdered, StreamExt},
};
use log::{debug, error, info, warn};
use std::collections::HashMap;

pub struct Server {
    listener: TcpListener,

    // Map topics to client ids.
    subscriptions: HashMap<String, (Sender<Packet>, Vec<String>)>,
}

impl Server {
    pub fn new(listener: TcpListener) -> Self {
        Self {
            listener,
            subscriptions: HashMap::default(),
        }
    }

    // Process an event from a client
    async fn handle_client_message(&mut self, message: Message) -> Result<(), SendError<Packet>> {
        match message {
            Message::Connect(client_id, sender) => {
                if self
                    .subscriptions
                    .insert(client_id.clone(), (sender, vec![]))
                    .is_some()
                {
                    info!("{client_id} - Reconnected");
                };
            }

            Message::Packet(client_id, Packet::Subscribe(subscribe)) => {
                let Some((_, topics)) = self.subscriptions.get_mut(&client_id) else {
                    error!("{client_id} - SUBSCRIBE packet for an unknown client.");
                    return Ok(());
                };

                for (topic, _) in subscribe.topics() {
                    topics.push(topic.to_owned());
                }
            }
            Message::Packet(_, Packet::Publish(publish)) => {
                let needle = publish.topic();
                for (peer, topics) in self.subscriptions.values() {
                    if topics.iter().any(|topic| topic == needle) {
                        // TODO: remove client if sending fails
                        peer.send(Packet::Publish(publish.clone())).await?;
                    }
                }
            }

            _ => {}
        };
        Ok(())
    }

    pub async fn run(mut self) {
        let listener = self.listener.clone();
        let (tx_inbound, rx_inbound) = async_channel::bounded::<Message>(100);

        let future1 = async {
            loop {
                futures::select! {
                    message = rx_inbound.recv().fuse() => {
                        match message {
                            Ok(message) => _ = self.handle_client_message(message).await,
                            Err(error) => {
                                error!("Fatal error, the receiver died: {error:?}");
                                return
                            }
                        }
                    }
                }
            }
        };

        let future2 = async {
            let mut futures = FuturesOrdered::new();
            let (stream, _) = listener
                .accept()
                .await
                .expect("Server failed to accept new connections.");

            futures.push_back(handle_client(stream, tx_inbound.clone()));

            loop {
                futures::select! {
                    peer  = listener.accept().fuse() => {
                        match peer {
                            Ok((stream, _)) => {
                                futures.push_back(handle_client(stream, tx_inbound.clone()));
                            }
                            Err(error) => {
                                panic!("Failed to connect new clients: {error:?}");
                            }
                        }
                    }
                    _ = futures.next() => {}
                }
            }
        };
        smol::pin!(future1);
        smol::pin!(future2);
        let mut future1 = future1.fuse();
        let mut future2 = future2.fuse();

        futures::select! {
            _ = future1 => {
                panic!("Fatal error when processing messages")
            }
            _ = future2 => {
                panic!("Fatal error when reading from socket(s).")
            }
        };
    }
}

async fn handle_client(mut stream: TcpStream, sender: Sender<Message>) {
    let packet = match read_packet(&mut stream).await {
        Ok(packet) => packet,
        Err(error) => {
            error!("Failed to read MQTT packet, closing connection: {error:?}");
            return;
        }
    };

    let Packet::Connect(connect) = &packet else {
        error!("Client did not set CONNECT as first message, instead  it sent  {packet:?}");
        return;
    };
    let client_id = connect.client_id().to_owned();
    debug!("{client_id} <-- {packet:?}");

    let ack = ConnAck::builder()
        .session_present()
        .return_code(ReturnCode::ConnectionAccepted)
        .build();

    if let Err(error) = stream.write_all(ack.as_bytes()).await {
        error!("{client_id} - Failed to write CONNACK packet, closing connection: {error:?}");
        return;
    };

    let (tx_outbound, rx_outbound) = async_channel::bounded::<Packet>(100);
    if let Err(error) = sender
        .send(Message::Connect(client_id.clone(), tx_outbound))
        .await
    {
        panic!("Failed to internally forward MQTT packet. That's a fatal error : {error:?}");
    }

    loop {
        let future2 = rx_outbound.recv();
        smol::pin!(future2);
        let mut future2 = future2.fuse();

        futures::select! {
            packet = read_packet(&mut stream).fuse() =>  {
                let packet = match packet {
                    Ok(packet) => packet,
                    Err(error) => {
                        error!("Failed to read packet from stream, closing connection for this client: {error:?}");
                        return
                    }
                };
                info!("{client_id} <-- {packet:?}");

                let packet = match packet {
                    Packet::PingReq(..) => Some(Packet::PingResp(PingResp)),
                    Packet::Disconnect(..) => return,
                    Packet::Subscribe(subscribe) => {
                        let mut topics = subscribe.topics();

                        // This should not panic, as subscribe must contain 1 topic.
                        let (_, qos) = topics.next().unwrap();

                        let mut builder = SubAck::builder(subscribe.packet_identifier(), qos);
                        for (_, qos) in topics {
                            builder = builder.add_return_code(qos);
                        }
                        if let Err(error) = sender
                            .send(Message::Packet(client_id.clone(), Packet::Subscribe(subscribe)))
                            .await {
                                panic!("Failed to internally forward MQTT packet. That's a fatal error : {error:?}");
                        }

                        Some(builder.build_packet())
                    }
                    Packet::Publish(publish) => {
                        if let Err(error) = sender
                            .send(Message::Packet(client_id.clone(), Packet::Publish(publish)))

                            .await {
                                panic!("Failed to internally forward MQTT packet. That's a fatal error : {error:?}");
                        }
                        None
                    }
                    Packet::Connect(..) | Packet::SubAck(..) | Packet::PubAck(..) => {
                        warn!("Client send packet only a broker is allowed to send, closing connection.");
                        return
                    }

                    other => todo!("{other:?} is not yet implemented"),
                };

                if let Some(packet) = packet
                    && let Err(error) = stream.write_all(&packet.into_bytes()).await {
                        warn!("Failed to send packet to Client, the connection is gone: {error:?}");
                        return
                    }
            },
            packet = future2 => {
                match packet {
                    Ok(packet) => {
                        if let Err(error) = stream.write_all(&packet.into_bytes()).await {
                            warn!("Failed to send packet to Client, the connection is gone: {error:?}");
                            return
                        }
                    }
                    Err(error) => {
                        warn!("{client_id} - connection lost: {error:?}");
                        return
                    }

                }

            }

        }
    }
}

async fn read_packet<R>(reader: &mut R) -> Result<Packet, DecodingError>
where
    R: AsyncRead + Unpin,
{
    let mut parser = Parser::new();
    loop {
        let bytes_required = parser.bytes_required() as usize;
        if bytes_required == 0 {
            return parser.parse();
        }

        let mut buf = vec![0; bytes_required];
        if let Err(error) = reader.read_exact(&mut buf).await {
            error!("Failed to read data from client's TCP connection: {error:?}");
            return Err(DecodingError::TooManyBytes);
        }
        parser.push(&buf);
    }
}

struct Parser {
    inner: BytesMut,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            inner: BytesMut::new(),
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.inner.put(data);
    }

    pub fn bytes_required(&self) -> u32 {
        packet::min_bytes_required(&self.inner)
    }

    pub fn parse(&mut self) -> Result<Packet, DecodingError> {
        match Packet::try_from(self.inner.clone().freeze()) {
            Ok(packet) => {
                self.inner = BytesMut::new();
                Ok(packet)
            }
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone)]
enum Message {
    Connect(String, Sender<Packet>),
    Packet(String, Packet),
}
