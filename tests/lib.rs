use async_net::{TcpListener, TcpStream};
use broker::Broker;
use bytes::Bytes;
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
use pretty_assertions::assert_eq;
use smol::Timer;
use std::{future, time::Duration};
use tjiftjaf::{Client, Frame, Options, Packet, PacketType, Publish, packet_v2::connack::ConnAck};
mod broker;
use macro_rules_attribute::apply;
use smol_macros::test;

const TOPIC: &'static str = "topic";

async fn create_client(port: u16) -> Client<TcpStream> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to open TCP connection to broker.");

    let options = Options {
        client_id: Some("test".to_string()),
        keep_alive: 5,
        ..Options::default()
    };
    Client::new(options, stream)
}

// Connect a client to a broker.
// Then, subscribe to a topic and publish to that same topic.
// Verify that the client receives published message.
#[apply(test!)]
async fn test_subcribe_and_publish() {
    let broker = Broker::new();
    let (mut handle_a, task) = create_client(broker.port).await.spawn();
    let _handle = smol::spawn(task);

    // After connecting, the broker returns a CONNACK packet.
    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::ConnAck);

    handle_a.subscribe(TOPIC).await.unwrap();
    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::SubAck);

    handle_a
        .publish(TOPIC, Bytes::from_static(b"test_subscribe_and_publish"))
        .await
        .unwrap();

    let publish = match handle_a.any_packet().await.unwrap() {
        Packet::Publish(publish) => publish,
        _ => panic!("Invalid packet."),
    };
    assert_eq!(publish.topic(), TOPIC);
    assert_eq!(publish.payload(), b"test_subscribe_and_publish");

    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::PingResp);
}

// Issue #17 tracked a bug where `MqttBinding` failed to
// decode a MQTT packet that was segmented over multiple TCP frames.
//
// This test verifies the fix for that bug.
//
// The test spawns a custom broker that emits a Publish packet
// that's split in 2 TCP frame. The frames are some time apart.
// This interval allows the `Client` to process each TCP frames separatly.
#[apply(test!)]
async fn test_17_decoding_large_packets() {
    let server = TcpListener::bind("localhost:0").await.unwrap();
    let client = create_client(server.local_addr().unwrap().port());

    // A task where the `server` accepts an incoming connection.
    // After the CONNECT/CONNACK exchange,  the server emits a PUBLISH
    // packet that's split into 2 TCP frames.
    let _server = smol::spawn(async move {
        let mut stream = server.incoming().next().await.unwrap().unwrap();
        let mut buf = vec![0u8; 1024];

        dbg!(stream.read(&mut buf).await.unwrap());
        let packet = ConnAck::builder().build();
        stream.write_all(&Bytes::from(packet)).await.unwrap();

        let packet = Publish::builder()
            .topic(TOPIC.to_string())
            .payload(Bytes::from_static(b"test_subscribe_and_publish"))
            .build();

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

    let (mut handle_a, task) = client.await.spawn();
    let _handle = smol::spawn(task);

    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::ConnAck);

    let publish = match handle_a.any_packet().await.unwrap() {
        Packet::Publish(publish) => publish,
        _ => panic!("Invalid packet."),
    };
    assert_eq!(publish.topic(), TOPIC);

    // Verify that the packet contains the expected payload.
    assert_eq!(publish.payload(), b"test_subscribe_and_publish");
}
