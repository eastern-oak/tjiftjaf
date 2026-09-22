mod env;

#[cfg(feature = "async")]
mod aio {
    use crate::env::broker::Broker;
    use crate::env::wiretap::{spawn_wiretapped_client, Line, Transcription};
    use async_channel::Sender;
    use async_net::{TcpListener, TcpStream};
    use futures_lite::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
    use log::debug;
    use macro_rules_attribute::apply;
    use smol::Timer;
    use smol_macros::test;
    use std::{
        collections::VecDeque,
        future,
        io::{self, ErrorKind},
        task::Poll,
        time::Duration,
    };
    use tjiftjaf::{
        aio::{Client, ClientHandle, Emit},
        publish, subscribe, ConnAck, Connect, Frame, Packet, PacketType, Publish, Subscribe,
    };

    #[cfg(feature = "experimental")]
    use tjiftjaf::aio::server::Server;

    const TOPIC: &str = "topic";

    // Create a `Client` and open a connection to the given port.
    // The `Client` runs in an isolated task and a handle to the client is
    // returned.
    async fn spawn_client(port: u16) -> ClientHandle {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("Failed to open TCP connection to broker.");

        let connect = Connect::builder()
            .client_id(stream.local_addr().unwrap().port())
            .keep_alive(5)
            .build()
            .unwrap();
        let (mut client, handle) = Client::new(connect);
        smol::spawn(async move { client.run(stream).await }).detach();
        handle
    }

    /// An implementation of `AsyncRead` and `AsyncWrite` that deliberetly fails
    /// after a certain number of reads or writes. It is used to verify
    #[derive(Debug, Clone)]
    struct FlakyConnection {
        // These packets are to generate response for `FlakyConnection.poll_read()`.
        // Since single packet are usually not read in a single `poll_read()` operation, `partial_read` includes
        // the portion of packet that hasn't been read yet.
        reads: VecDeque<Packet>,
        partial_read: Option<Vec<u8>>,

        // `FlakyConnection.poll_write()` returns an error after a number of calls.
        // `writes` keeps track of how many times `poll_write()` is called. Whereas `max_writes`
        // holds the maximum number of calls to `poll_write()` are allowed before returning an error.
        writes: usize,
        max_writes: usize,

        history: Sender<Line>,
    }

    impl FlakyConnection {
        pub fn new(reads: Vec<impl Into<Packet>>, max_writes: usize) -> (Self, Transcription) {
            let history = Transcription::new();

            let this = Self {
                reads: reads.into_iter().map(|read| read.into()).collect(),
                partial_read: None,
                writes: 0,
                max_writes,
                history: history.handler(),
            };
            (this, history)
        }
    }

    impl AsyncRead for FlakyConnection {
        /// Return one item from self.reads.
        /// If self.reads is empty, return an error
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            let self_mut = unsafe { self.get_unchecked_mut() };

            let read = match self_mut.partial_read.take() {
                Some(read) => read,
                None => {
                    let Some(read) = self_mut.reads.pop_front() else {
                        debug!("FlakyConnection.poll_read() failed");
                        return Poll::Ready(Err(io::Error::from(ErrorKind::PermissionDenied)));
                    };
                    debug!("FlakyConnection.poll_read() {:?}", read);
                    read.into_bytes()
                }
            };

            let n = buf.len().min(read.len());

            debug!("FlakyConnection.poll_read() {:?}", &read[..n]);
            buf[..n].copy_from_slice(&read[..n]);
            if read[n..].len() > 0 {
                self_mut.partial_read = Some(read[n..].to_vec());
            }
            Poll::Ready(Ok(n))
        }
    }

    impl AsyncWrite for FlakyConnection {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.writes >= self.max_writes {
                debug!("FlakyConnection.poll_write() failed");
                return Poll::Ready(Err(io::Error::from(ErrorKind::AddrInUse)));
            }
            debug!("FlakyConnection.poll_write() {:?}", &buf[..]);

            let self_mut = unsafe { self.get_unchecked_mut() };

            let packet = Packet::try_from(buf.to_vec()).unwrap_or_else(|error| {
                panic!("FlakyConnection failed to parse a written packet: {error:?}")
            });
            self_mut
                .history
                .try_send(Line::Client(packet))
                .unwrap_or_else(|e| {
                    panic!("Failed to record the client's payload in the transcription: {e:?}")
                });
            self_mut.writes += 1;

            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    // Connect a client to a broker.
    // Then, subscribe to a topic and publish to that same topic.
    // Verify that the client receives published message.
    #[apply(test!)]
    async fn test_subscribe_and_publish() {
        let broker = Broker::new();
        let (handle, mut history) = spawn_wiretapped_client(broker.port).await;

        // After connecting, the broker returns a CONNACK packet.
        let _ = history.find(PacketType::ConnAck).await;

        subscribe(TOPIC).unwrap().emit(&handle).await.unwrap();
        let _ = history.find(PacketType::SubAck).await;

        publish(TOPIC, "test_subscribe_and_publish")
            .unwrap()
            .emit(&handle)
            .await
            .unwrap();

        let packet = history.find(PacketType::Publish).await;
        let Packet::Publish(publish) = packet else {
            panic!();
        };
        assert_eq!(publish.topic(), TOPIC);
        assert_eq!(publish.payload(), b"test_subscribe_and_publish");

        // TODO GH-118: When uncommented, this line causes the test to become
        // flaky.
        // let packet = history.find(PacketType::PinResp).await;

        handle.disconnect().await.unwrap();
        let _ = history.find(PacketType::Disconnect).await;
        // TODO: how to check that client stopped.
        // assert!(_handle.await.is_ok());
    }

    // Issue #17 tracked a bug where `MqttBinding` failed to
    // decode a MQTT packet that was segmented over multiple TCP frames.
    //
    // This test verifies the fix for that bug.
    //
    // The test spawns a custom broker that emits a Publish packet
    // that's split in 2 TCP frame. The frames are some time apart.
    // This interval allows the `Client` to process each TCP frames separately.
    #[apply(test!)]
    async fn test_17_decoding_large_packets() {
        let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut handle = spawn_client(server.local_addr().unwrap().port()).await;

        // A task where the `server` accepts an incoming connection.
        // After the CONNECT/CONNACK exchange, the server emits a PUBLISH
        // packet that's split into 2 TCP frames.
        let _server = smol::spawn(async move {
            let mut stream = server.incoming().next().await.unwrap().unwrap();
            let mut buf = vec![0u8; 1024];

            stream.read(&mut buf).await.unwrap();
            let packet = ConnAck::builder().build();
            stream.write_all(packet.as_bytes()).await.unwrap();

            let packet = Publish::builder(TOPIC, "test_subscribe_and_publish")
                .build()
                .unwrap();

            let split_at = packet.length() as usize - 5;

            stream
                .write_all(&packet.as_bytes()[0..split_at])
                .await
                .unwrap();
            stream.flush().await.unwrap();
            Timer::after(Duration::from_secs(1)).await;

            stream
                .write_all(&packet.as_bytes()[split_at..])
                .await
                .unwrap();

            let () = future::pending().await;
        });

        // let packet = handle_a.any_packet().await.unwrap();
        // assert_eq!(packet.packet_type(), PacketType::ConnAck);

        let publish = handle.subscriptions().await.unwrap();

        assert_eq!(publish.topic(), TOPIC);
        assert_eq!(publish.payload(), b"test_subscribe_and_publish");
    }

    // When a peer emits a PUBLISH with QOS of 1, the receiver must acknowledge
    // this message with a PUBACK.
    //
    // When peer A emits a PUBLISH with QOS of 2, peer B must acknowledge
    // the message with a PUBREC. In turn, the peer A acknowledges the PUBREC
    // by emitting a PUBREL. Lastly, peer B must acknowledge that message
    // using a PUBCOMP.
    //
    // This test verifies that both sequences are implemented correctly.
    #[apply(test!)]
    async fn test_qos_1_and_qos_2() {
        let broker = Broker::new();
        let (handle, mut history) = spawn_wiretapped_client(broker.port).await;

        // After connecting, the broker returns a CONNACK packet.
        let _ = history.find(PacketType::ConnAck).await;

        Subscribe::builder(TOPIC, tjiftjaf::QoS::AtLeastOnceDelivery)
            .build()
            .unwrap()
            .emit(&handle)
            .await
            .unwrap();
        let _ = history.find(PacketType::SubAck).await;

        publish(TOPIC, "test_subscribe_and_publish")
            .unwrap()
            .emit(&handle)
            .await
            .unwrap();

        let _ = history.find(PacketType::Publish).await;
        let _ = history.find(PacketType::PubAck).await;

        // Now subscribe with QoS of 2.
        Subscribe::builder(TOPIC, tjiftjaf::QoS::ExactlyOnceDelivery)
            .build()
            .unwrap()
            .emit(&handle)
            .await
            .unwrap();
        let _ = history.find(PacketType::SubAck).await;

        Publish::builder(TOPIC, "yolo")
            .qos(tjiftjaf::QoS::ExactlyOnceDelivery)
            .build()
            .unwrap()
            .emit(&handle)
            .await
            .unwrap();

        let _ = history.find(PacketType::PubRec).await;
        let _ = history.find(PacketType::PubRel).await;
        let _ = history.find(PacketType::PubComp).await;
    }

    #[cfg(feature = "experimental")]
    #[apply(test!)]
    async fn test_client_and_server() {
        simple_logger::init_with_level(log::Level::Debug).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = Server::new(listener);
        let _server_handle = smol::spawn(server.run());

        let mut handle_1 = spawn_client(local_addr.port()).await;
        let handle_2 = spawn_client(local_addr.port()).await;

        Subscribe::builder("test/#", tjiftjaf::QoS::AtLeastOnceDelivery)
            .build()
            .unwrap()
            .emit(&handle_1)
            .await
            .unwrap();

        publish("test/client_and_server", "test_subscribe_and_publish")
            .unwrap()
            .emit(&handle_2)
            .await
            .unwrap();

        let publication = handle_1.subscriptions().await.unwrap();
        assert_eq!(&publication.topic(), &"test/client_and_server");
        assert_eq!(&publication.payload(), b"test_subscribe_and_publish");
    }

    /// Verify that outbound packets are _not_ lost when the connection
    /// breaks while sending a packet.
    #[apply(test!)]
    async fn test_retransmitting_packet_when_connection_is_breaks() {
        let connack = ConnAck::builder().build();

        let (stream, mut history) = FlakyConnection::new(vec![connack.clone()], 1);
        let (mut client, handle) = Client::new(Connect::builder().build().unwrap());
        subscribe("sensors/1").unwrap().emit(&handle).await.unwrap();

        assert!(client.run(stream).await.is_err());

        // The CONNECT packet was sent successfully before the connection died,
        // butthe SUBSCRIBE packet never made it onto the wire.
        let _ = history.find(PacketType::Connect).await;
        assert!(history
            .try_find_with(|packet| packet.packet_type() == PacketType::Subscribe)
            .await
            .is_err());

        let (stream, mut history) = FlakyConnection::new(vec![connack], 2);
        assert!(client.run(stream).await.is_err());

        // After reconnecting, the client must resend the CONNECT packet as well as
        // retransmit the SUBSCRIBE packet that was lost earlier.
        let _ = history.find(PacketType::Connect).await;
        let _ = history.find(PacketType::Subscribe).await;
    }
}

#[cfg(feature = "blocking")]
mod blocking {
    use async_channel::Sender;
    use macro_rules_attribute::apply;
    use pretty_assertions::assert_eq;
    use smol_macros::test;
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpStream},
        os::unix::net::UnixStream,
        thread,
        time::Duration,
    };
    use tjiftjaf::{
        blocking::{self, Emit},
        packet, publish, subscribe, ConnAck, Connect, Frame, Packet, PacketType,
    };

    use crate::env::wiretap::{Line, Transcription};
    const TOPIC: &str = "topic";

    // Create a `Client` and open a connection to the given port.
    // The `Client` runs in an isolated thread and a handle to the client is
    // returned.
    fn spawn_blocking_client(port: u16) -> blocking::ClientHandle {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .expect("Failed to open TCP connection to broker.");

        let connect = Connect::builder()
            .client_id("test")
            .keep_alive(5)
            .build()
            .unwrap();
        let (mut client, handle) = blocking::Client::new(connect).unwrap();
        thread::spawn(move || client.run_from_std(stream));
        handle
    }

    // Connect a client to a broker.
    // Then, subscribe to a topic and publish to that same topic.
    // Verify that the client receives published message.
    #[test]
    fn test_subscribe_and_publish_with_blocking_client() {
        use crate::env::broker::Broker;

        let broker = Broker::new();
        let mut handle_a = spawn_blocking_client(broker.port);

        subscribe(TOPIC).unwrap().emit(&handle_a).unwrap();

        // Until GH-71 is implemented, we need to introduce an artificial
        // sleep.
        //
        // https://github.com/eastern-oak/tjiftjaf/issues/71
        std::thread::sleep(Duration::from_secs(1));

        publish(TOPIC, "test_subscribe_and_publish")
            .unwrap()
            .emit(&handle_a)
            .unwrap();

        let publish = handle_a.publication().unwrap();

        assert_eq!(publish.topic(), TOPIC);
        assert_eq!(publish.payload(), b"test_subscribe_and_publish");

        handle_a.disconnect().unwrap();
        // TODO: how to check that client stopped.
        // assert!(task.join().is_ok());
    }

    /// A fake broker that allows us to break the connection to the client
    /// deterministically.
    struct FlakyBroker {
        // The broker tracks how many packet is has received from the client.
        // The broker break the connnection if that number reaches `max_writes`.
        writes: usize,
        max_writes: usize,

        history: Sender<Line>,
    }

    impl FlakyBroker {
        pub fn new(max_writes: usize) -> (Self, Transcription) {
            let history = Transcription::new();

            let this = Self {
                writes: 0,
                max_writes,
                history: history.handler(),
            };
            (this, history)
        }

        /// Start the broker and return a stream. The stream must
        /// be used by the `blocking::Client`. to interact with the broker.
        fn serve_and_connect(mut self) -> mio::net::UnixStream {
            let (client_end, mut broker_end) =
                UnixStream::pair().expect("Failed to create a UnixStream pair.");

            thread::spawn(move || loop {
                let packet = read_packet(&mut broker_end);

                if let Packet::Connect(_) = &packet {
                    let connack = ConnAck::builder().build();
                    broker_end.write_all(connack.as_bytes()).unwrap();
                }

                self.writes += 1;
                self.history.send_blocking(Line::Client(packet)).unwrap();

                if self.writes >= self.max_writes {
                    let _ = broker_end.shutdown(Shutdown::Both);
                    break;
                }
            });

            mio::net::UnixStream::from_std(client_end)
        }
    }

    // Read exactly one `Packet` from `stream`, blocking until it has fully
    // arrived (it may be split over multiple frames).
    fn read_packet(stream: &mut UnixStream) -> Packet {
        let mut buf = Vec::new();
        loop {
            let required = packet::min_bytes_required(&buf) as usize;
            if required == 0 {
                break;
            }

            let mut chunk = vec![0; required];
            stream
                .read_exact(&mut chunk)
                .expect("FakeBroker failed to read a packet from the client.");
            buf.extend_from_slice(&chunk);
        }

        Packet::try_from(buf).expect("FakeBroker failed to parse a packet sent by the client.")
    }

    /// Verify that outbound packets are _not_ lost when the connection
    /// breaks
    #[apply(test!)]
    async fn test_retransmitting_packet_when_connection_breaks() {
        simple_logger::init_with_level(log::Level::Debug).unwrap();
        let (broker, mut history) = FlakyBroker::new(1);

        let (mut client, handle) =
            blocking::Client::new(Connect::builder().build().unwrap()).unwrap();
        let socket = broker.serve_and_connect();
        subscribe("sensors/1").unwrap().emit(&handle).unwrap();

        assert!(client.run(socket).is_err());

        // The CONNECT packet was sent successfully before the connection died,
        // but the SUBSCRIBE packet never made it onto the wire.
        let _ = history.find(PacketType::Connect).await;
        assert!(history
            .try_find_with(|packet| packet.packet_type() == PacketType::Subscribe)
            .await
            .is_err());

        let (broker, mut history) = FlakyBroker::new(2);
        let socket = broker.serve_and_connect();
        assert!(client.run(socket).is_err());

        // After reconnecting, the client must resend the CONNECT packet as well as
        // retransmit the SUBSCRIBE packet that was lost earlier.
        let _ = history.find(PacketType::Connect).await;
        let _ = history.find(PacketType::Subscribe).await;
    }
}
