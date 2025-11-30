use crate::{
    ConnAck, Connect, DecodingError, Frame, Packet, PingResp, SubAck,
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

    // Map client ids to topics.
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
            Message::Register(client_id, sender) => {
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
                let mut disconnected_clients: Vec<String> = Vec::new();
                let needle = publish.topic();
                let subscriptions = self.subscriptions.iter().filter(|(_, (_, topics))| {
                    topics
                        .iter()
                        .any(|subscription| does_topic_match_subscription(subscription, needle))
                });

                for (client_id, (peer, _)) in subscriptions {
                    if let Err(error) = peer.send(Packet::Publish(publish.clone())).await {
                        warn!("{client_id} - Failed to send packet: {error:?}");
                        disconnected_clients.push(client_id.clone());
                    };
                }

                for client in disconnected_clients {
                    self.subscriptions.remove(&client);
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

            futures.push_back(on_new_connection(stream, tx_inbound.clone()));

            loop {
                futures::select! {
                    peer  = listener.accept().fuse() => {
                        match peer {
                            Ok((stream, _)) => {
                                futures.push_back(on_new_connection(stream, tx_inbound.clone()));
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

async fn on_new_connection(stream: TcpStream, funnel: Sender<Message>) -> Result<(), ClientError> {
    let client = AnonymousClient::new(stream);
    let mut client = client.deanonymize().await?;
    client
        .run(funnel)
        .await
        .inspect(|_| info!("{} disconnected", client.client_id()))
        .inspect_err(|error| error!("{} disconnected: {error:?}", client.client_id()))
}

#[derive(Debug)]
enum ClientError {
    // Something went wrong while interacting with the socket.
    IoError(std::io::Error),

    // Failed to decode an inbound packet.
    DecodingError(DecodingError),

    ServerError,

    // The server received a packet is it didn't expect. For example,
    // a second CONNECT packet, a CONNACK, a SUBACK, etc.
    UnexpectedPacket,
}

impl From<DecodingError> for ClientError {
    fn from(value: DecodingError) -> Self {
        Self::DecodingError(value)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

impl From<SendError<Message>> for ClientError {
    fn from(_: SendError<Message>) -> Self {
        Self::ServerError
    }
}

struct AnonymousClient {
    stream: TcpStream,
}

impl AnonymousClient {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }

    pub async fn deanonymize(mut self) -> Result<Client, ClientError> {
        let packet = read_packet(&mut self.stream).await?;
        let Packet::Connect(connect) = packet else {
            return Err(ClientError::UnexpectedPacket);
        };
        let client_id = connect.client_id().to_owned();
        debug!("{client_id} <-- {connect:?}");

        let ack = ConnAck::builder()
            .return_code(ReturnCode::ConnectionAccepted)
            .build();

        info!("{client_id} --> {ack:?}");
        self.stream.write_all(ack.as_bytes()).await?;
        Ok(Client::new(self.stream, connect))
    }
}

struct Client {
    stream: TcpStream,
    connect: Connect,
}

impl Client {
    pub fn new(stream: TcpStream, connect: Connect) -> Self {
        Self { stream, connect }
    }

    pub fn client_id(&self) -> &str {
        self.connect.client_id()
    }

    // Send a packet to the client.
    async fn send(&mut self, packet: Packet) -> Result<(), ClientError> {
        info!("{} --> {packet:?}", self.client_id());
        self.stream.write_all(&packet.into_bytes()).await?;
        Ok(())
    }

    // Start the client. It'll perform 2 tasks in parallel:
    // * reading packets from the tcp stream and forwarding some to `inbound` channel.
    // * reading outbound packets from `receiver` and write them to the tcp stream.
    pub async fn run(&mut self, funnel: Sender<Message>) -> Result<(), ClientError> {
        let (tx, rx) = async_channel::bounded(100);
        funnel
            .send(Message::Register(self.client_id().to_owned(), tx))
            .await?;

        loop {
            let future2 = rx.recv();
            smol::pin!(future2);
            let mut future2 = future2.fuse();

            futures::select! {
                packet = read_packet(&mut self.stream).fuse() =>  {
                    let packet = packet?;
                    info!("{} <-- {packet:?}", self.client_id());

                    let packet = match packet {
                        Packet::PingReq(..) => Some(Packet::PingResp(PingResp)),
                        Packet::Disconnect(..) => {
                            info!("{} Client disconnected deliberedly.", self.client_id());
                            return Ok(());
                        }
                        Packet::Subscribe(subscribe) => {
                            let mut topics = subscribe.topics();

                            // This should not panic, as subscribe must contain 1 topic.
                            let (_, qos) = topics.next().unwrap();

                            let mut builder = SubAck::builder(subscribe.packet_identifier(), qos);
                            for (_, qos) in topics {
                                builder = builder.add_return_code(qos);
                            }
                            funnel
                                .send(Message::Packet(self.client_id().to_owned(), Packet::Subscribe(subscribe)))
                                .await?;
                            Some(builder.build_packet())
                        }

                        Packet::Publish(publish) => {
                            funnel
                                .send(Message::Packet(self.client_id().to_owned(), Packet::Publish(publish)))
                                .await?;
                            None
                        }
                        Packet::Connect(..) | Packet::SubAck(..) | Packet::PubAck(..) => {
                            warn!("Client sent packet only a broker is allowed to send, closing connection.");
                            return Err(ClientError::UnexpectedPacket);
                        }

                        other => todo!("{other:?} is not yet implemented"),
                    };

                    if let Some(packet) = packet {
                        self.send(packet).await?;
                    }
                },
                packet = future2 => {
                    match packet {
                        Ok(packet) => {
                            self.send(packet).await?;
                        }
                        Err(error) => {
                            warn!("{} - connection lost: {error:?}", self.client_id());
                            return Err(ClientError::ServerError);
                        }
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

    fn bytes_required(&self) -> u32 {
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
    Register(String, Sender<Packet>),
    Packet(String, Packet),
}

// Verify if a topic match a subscription. The subscription may
// include wildcards like `#` and `+`.
fn does_topic_match_subscription(subscription: &str, topic: &str) -> bool {
    // If no wild cards are used, check for exact match
    if !subscription.contains('#') && !subscription.contains('+') {
        return subscription == topic;
    }

    if let Some((prefix, _)) = subscription.split_once('#') {
        return topic.starts_with(prefix);
    }

    let mut topic_segments = topic.split('/');

    for filter in subscription.split('/') {
        // The topic and a subscription using `+` must have the same
        // number of segments. If the topic has less segments, it is no match.
        let Some(segment) = topic_segments.next() else {
            return false;
        };

        if filter == "+" {
            continue;
        }

        if filter != segment {
            return false;
        }
    }

    // The topic and a subscription using `+` must have the same
    // number of segments. If the topic has more segments, it is no match.
    if topic_segments.next().is_some() {
        return false;
    }

    true
}

#[cfg(test)]
mod test {
    use super::does_topic_match_subscription;

    #[test]
    fn test_does_topic_match_subscription() {
        assert!(does_topic_match_subscription(
            "sensors/3/value",
            "sensors/3/value"
        ));

        assert!(does_topic_match_subscription(
            "sensors/+/value",
            "sensors/3/value"
        ));

        assert!(does_topic_match_subscription(
            "sensors/+/+",
            "sensors/3/value"
        ));

        assert!(does_topic_match_subscription(
            "sensors/#",
            "sensors/3/value"
        ));

        // These topics don't match
        assert!(!does_topic_match_subscription(
            "sensors/3/value",
            "sensors/1/value"
        ));

        assert!(!does_topic_match_subscription(
            "sensors/+/value",
            "sensors/1/name"
        ));
    }
}
