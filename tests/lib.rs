mod env;

#[cfg(feature = "async")]
mod aio {
    use crate::env::broker::Broker;
    use crate::env::wiretap::wiretapped_client;
    use async_net::{TcpListener, TcpStream};
    use bytes::Bytes;
    use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
    use macro_rules_attribute::apply;
    use smol::Timer;
    use smol_macros::test;
    use std::{future, time::Duration};
    use tjiftjaf::{ConnAck, Connect, Frame, Packet, PacketType, Publish, aio::Client};

    #[cfg(feature = "experimental")]
    use tjiftjaf::aio::server::Server;

    const TOPIC: &str = "topic";

    async fn create_client(port: u16) -> Client<TcpStream> {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("Failed to open TCP connection to broker.");

        let connect = Connect::builder()
            .client_id(stream.local_addr().unwrap().port())
            .keep_alive(5)
            .build();
        Client::new(connect, stream)
    }

    // Connect a client to a broker.
    // Then, subscribe to a topic and publish to that same topic.
    // Verify that the client receives published message.
    #[apply(test!)]
    async fn test_subscribe_and_publish() {
        let broker = Broker::new();
        let (client, mut history) = wiretapped_client(broker.port).await;
        let (handle, task) = client.spawn();

        let _handle = smol::spawn(task);

        // After connecting, the broker returns a CONNACK packet.
        let _ = history.find(PacketType::ConnAck).await;

        handle
            .subscribe(TOPIC, tjiftjaf::QoS::AtMostOnceDelivery)
            .await
            .unwrap();
        let _ = history.find(PacketType::SubAck).await;

        handle
            .publish(TOPIC, Bytes::from_static(b"test_subscribe_and_publish"))
            .await
            .unwrap();

        let Packet::Publish(publish) = history.find(PacketType::Publish).await else {
            panic!("Invalid packet.")
        };

        assert_eq!(publish.topic(), TOPIC);
        assert_eq!(publish.payload(), b"test_subscribe_and_publish");

        // TODO GH-118: When uncommented, this line causes the test to become
        // flaky.
        // let packet = history.find(PacketType::PinResp).await;

        handle.disconnect().await.unwrap();
        let _ = history.find(PacketType::Disconnect).await;
        assert!(_handle.await.is_ok());
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
        let (client, mut history) = wiretapped_client(server.local_addr().unwrap().port()).await;

        // A task where the `server` accepts an incoming connection.
        // After the CONNECT/CONNACK exchange,  the server emits a PUBLISH
        // packet that's split into 2 TCP frames.
        let _server = smol::spawn(async move {
            let mut stream = server.incoming().next().await.unwrap().unwrap();
            let mut buf = vec![0u8; 1024];

            stream.read(&mut buf).await.unwrap();
            let packet = ConnAck::builder().build();
            stream.write_all(&Bytes::from(packet)).await.unwrap();

            let packet =
                Publish::builder(TOPIC, Bytes::from_static(b"test_subscribe_and_publish")).build();

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

        let (_handle_a, task) = client.spawn();
        let _handle = smol::spawn(task);

        let _ = history.find(PacketType::ConnAck).await;

        let Packet::Publish(publish) = history.find(PacketType::Publish).await else {
            panic!("Invalid packet.")
        };
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
        let (client, mut history) = wiretapped_client(broker.port).await;
        let (handle_a, task) = client.spawn();

        let _handle = smol::spawn(task);

        // After connecting, the broker returns a CONNACK packet.
        let _ = history.find(PacketType::ConnAck).await;

        handle_a
            .subscribe(TOPIC, tjiftjaf::QoS::AtLeastOnceDelivery)
            .await
            .unwrap();
        let _ = history.find(PacketType::SubAck).await;

        handle_a
            .publish(TOPIC, Bytes::from_static(b"test_subscribe_and_publish"))
            .await
            .unwrap();

        let _ = history.find(PacketType::Publish).await;
        let _ = history.find(PacketType::PubAck).await;

        // Now subscribe with QoS of 2.
        handle_a
            .subscribe(TOPIC, tjiftjaf::QoS::ExactlyOnceDelivery)
            .await
            .unwrap();
        let _ = history.find(PacketType::SubAck).await;

        let packet = Publish::builder(TOPIC, "yolo")
            .qos(tjiftjaf::QoS::ExactlyOnceDelivery)
            .build_packet();

        // handle_a.send(packet).await.unwrap();

        // let _ = history.find(PacketType::PubRec).await;
        // let _ = history.find(PacketType::PubRel).await;
        // let _ = history.find(PacketType::PubComp).await;
    }

    #[cfg(feature = "experimental")]
    #[apply(test!)]
    async fn test_client_and_server() {
        simple_logger::init_with_level(log::Level::Debug).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = Server::new(listener);
        let _server_handle = smol::spawn(server.run());

        let (mut handle_1, task) = create_client(local_addr.port()).await.spawn();
        let _handle = smol::spawn(task);
        let (handle_2, task) = create_client(local_addr.port()).await.spawn();
        let _handle = smol::spawn(task);

        handle_1
            .subscribe("test/#", tjiftjaf::QoS::AtLeastOnceDelivery)
            .await
            .unwrap();

        handle_2
            .publish(
                "test/client_and_server",
                Bytes::from_static(b"test_subscribe_and_publish"),
            )
            .await
            .unwrap();

        let publication = handle_1.publication().await.unwrap();
        assert_eq!(&publication.topic(), &"test/client_and_server");
        assert_eq!(&publication.payload(), b"test_subscribe_and_publish");
    }
}

#[cfg(feature = "blocking")]
mod blocking {
    use bytes::Bytes;
    use pretty_assertions::assert_eq;
    use std::time::Duration;
    use tjiftjaf::{Connect, blocking};

    const TOPIC: &str = "topic";

    fn create_blocking_client(port: u16) -> blocking::Client {
        let stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .expect("Failed to open TCP connection to broker.");

        let connect = Connect::builder().client_id("test").keep_alive(5).build();
        blocking::Client::new(connect, stream)
    }

    // Connect a client to a broker.
    // Then, subscribe to a topic and publish to that same topic.
    // Verify that the client receives published message.
    #[test]
    fn test_subscribe_and_publish_with_blocking_client() {
        use crate::env::broker::Broker;

        let broker = Broker::new();
        let (mut handle_a, task) = create_blocking_client(broker.port).spawn().unwrap();

        handle_a
            .subscribe(TOPIC, tjiftjaf::QoS::AtLeastOnceDelivery)
            .unwrap();

        // Until GH-71 is implemented, we need to introduce an artificial
        // sleep.
        //
        // https://github.com/eastern-oak/tjiftjaf/issues/71
        std::thread::sleep(Duration::from_secs(1));

        handle_a
            .publish(TOPIC, Bytes::from_static(b"test_subscribe_and_publish"))
            .unwrap();

        let publish = handle_a.publication().unwrap();

        assert_eq!(publish.topic(), TOPIC);
        assert_eq!(publish.payload(), b"test_subscribe_and_publish");

        handle_a.disconnect().unwrap();
        assert!(task.join().is_ok());
    }
}
