use crate::env::broker::wait_server_listening;
use async_channel::{Receiver, Sender};
use async_net::{TcpListener, TcpStream};
use bytes::{BufMut, BytesMut};
use futures_lite::{AsyncReadExt, AsyncWriteExt, FutureExt, StreamExt};
use smol::spawn;
use tjiftjaf::{Connect, Packet, PacketType, asynchronous::Client, packet};

/// Start a proxy and connect `Client` through that proxy to the broker.
/// The interaction between `Client` and broker is recorded in a `Transcription`.
pub async fn wiretapped_client(port: u16) -> (Client<TcpStream>, Transcription) {
    // Bind the proxy to a random available port.
    let addr = "127.0.0.1:0";

    let proxy = TcpListener::bind(addr)
        .await
        .expect("Failed to bind proxy to a random port.");
    let proxy_port = proxy.local_addr().unwrap().port();

    let history = Transcription::new();
    let history_handler = history.handler();

    spawn(async move {
        // Ignore the first client that connects. That is the client created
        // by `wait_server_listening()`. This method knocks on the port of the server
        // to figure out if it's up.
        let _ = proxy.incoming().next().await.unwrap().unwrap();

        let mut client = proxy.incoming().next().await.unwrap().unwrap();
        // Open a connection to the broker.
        let mut broker = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("Failed to open TCP connection to broker.");

        enum Winner {
            // The client has some data for the broker.
            Client(Packet),

            // The broker has some data for the client.
            Broker(Packet),
        }

        let mut broker_parser = Parser::new();
        let mut client_parser = Parser::new();
        loop {
            let future_1 = async {
                loop {
                    let bytes_required = client_parser.bytes_required() as usize;
                    if bytes_required == 0 {
                        break;
                    }

                    let mut buf = vec![0; bytes_required];
                    client.read_exact(&mut buf).await.unwrap_or_else(|e| {
                        panic!("Failed to read data from client's TCP connection: {e:?}")
                    });
                    client_parser.push(&buf);
                }

                client_parser
                    .parse()
                    .map(Winner::Client)
                    .unwrap_or_else(|error| panic!("Wiretap failed to parse packet: {error:?}"))
            };
            let future_2 = async {
                loop {
                    let bytes_required = broker_parser.bytes_required() as usize;
                    if bytes_required == 0 {
                        break;
                    }

                    let mut buf = vec![0; bytes_required];
                    broker.read_exact(&mut buf).await.unwrap_or_else(|e| {
                        panic!("Failed to read data from broker's TCP connection: {e:?}")
                    });
                    broker_parser.push(&buf);
                }

                broker_parser
                    .parse()
                    .map(Winner::Broker)
                    .unwrap_or_else(|error| panic!("Wiretap failed to parse packet: {error:?}"))
            };

            let winner = future_1.race(future_2).await;
            match winner {
                Winner::Client(payload) => {
                    history_handler
                        .send(Line::Client(payload.clone()))
                        .await
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to record the client's payload in the transcription: {e:?}"
                            )
                        });
                    broker
                        .write(&payload.into_bytes())
                        .await
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to forward a payload to the broker's TCP connection: {e:?}"
                            )
                        });
                }

                Winner::Broker(payload) => {
                    history_handler
                        .send(Line::Broker(payload.clone()))
                        .await
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to record the broker's payload in the transcription: {e:?}"
                            )
                        });
                    client
                        .write(&payload.into_bytes())
                        .await
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to forward a payload to the client's TCP connection: {e:?}"
                            )
                        });
                }
            };
        }
    })
    .detach();

    // Wait until proxy is up before continuing.
    // In particular on slow machines this synchronization is important.
    wait_server_listening(proxy_port);

    let stream = TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
        .await
        .expect("Failed to open TCP connection to broker on port {proxy_port}.");

    let connect = Connect::builder().client_id("test").keep_alive(5).build();
    (Client::new(connect, stream), history)
}

/// A collection of all `Packet`s that are sent to the MQTT broker.
/// The requests are sorted by time of arrival.
///
/// To search the history for a certain request use any of `Transcription.find*()` methods.
pub struct Transcription {
    // Receiver for inbound messages sent by the MQTT client to the broker.
    inner: Receiver<Line>,

    // Sender that allows the proxy to append messages to the history.
    sender: Sender<Line>,

    // All logged `Packet`s.
    history: Vec<Packet>,
}

impl Transcription {
    /// Create a new `Transcription`.
    pub fn new() -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self {
            inner: rx,
            sender: tx,
            history: vec![],
        }
    }

    /// Obtain a sender to append messages to the history.
    pub fn handler(&self) -> Sender<Line> {
        self.sender.clone()
    }

    /// Find a `Packet` that matches the given path.
    /// When no immediate match is found, this method waits for new packets to arrive.
    pub async fn find(&mut self, packet_type: PacketType) -> Packet {
        self.find_with(|packet| packet.packet_type() == packet_type)
            .await
    }

    /// Find a `Packet` that matches the given predicate.
    /// When no immediate match is found, this method waits for new packets to arrive.
    pub async fn find_with<P>(&mut self, mut predicate: P) -> Packet
    where
        P: FnMut(&Packet) -> bool,
        P: Copy,
    {
        if let Ok(request) = self.try_find_with(predicate).await {
            return request;
        }

        loop {
            let packet = self
                .inner
                .recv()
                .await
                .map(|value| {
                    let direction = match value {
                        Line::Client(..) => "-->",
                        Line::Broker(..) => "<--",
                    };

                    let packet = value.into_packet();
                    println!("{direction} {:?}", packet);
                    packet
                })
                .unwrap();

            self.history.push(packet.clone());
            if predicate(&packet) {
                return packet;
            }
        }
    }

    /// Try find a `Packet` that matches the given predicate.
    pub async fn try_find_with<P>(&mut self, mut predicate: P) -> Result<Packet, NotFoundError>
    where
        P: FnMut(&Packet) -> bool,
    {
        while let Ok(request) = self.inner.try_recv().map(|value| {
            let direction = match value {
                Line::Client(..) => "-->",
                Line::Broker(..) => "<--",
            };

            let packet = value.into_packet();
            println!("{direction} {:?}", packet);
            packet
        }) {
            self.history.push(request.clone());
            if predicate(&request) {
                return Ok(request);
            }
        }

        Err(NotFoundError)
    }
}

pub enum Line {
    Client(Packet),
    Broker(Packet),
}

impl Line {
    fn into_packet(self) -> Packet {
        match self {
            Line::Client(packet) => packet,
            Line::Broker(packet) => packet,
        }
    }
}

#[derive(Debug)]
pub struct NotFoundError;

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

    pub fn parse(&mut self) -> Result<Packet, tjiftjaf::packet_v2::DecodingError> {
        match Packet::try_from(self.inner.clone().freeze()) {
            Ok(packet) => {
                self.inner = BytesMut::new();
                Ok(packet)
            }
            Err(error) => Err(error),
        }
    }
}
