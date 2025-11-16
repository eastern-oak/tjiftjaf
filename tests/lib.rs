use async_net::{TcpListener, TcpStream};
use bytes::Bytes;
use env::broker::Broker;
use env::wiretap::wiretapped_client;
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
use macro_rules_attribute::apply;
use pretty_assertions::assert_eq;
use smol::Timer;
use smol_macros::test;
use std::{future, time::Duration};
use tjiftjaf::{
    Frame, Packet, PacketType,
    asynchronous::Client,
    packet_v2::{connack::ConnAck, connect::Connect, publish::Publish},
};

#[cfg(feature = "blocking")]
use tjiftjaf::blocking;

mod env;

const TOPIC: &str = "topic";

async fn create_client(port: u16) -> Client<TcpStream> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to open TCP connection to broker.");

    let connect = Connect::builder().client_id("test").keep_alive(5).build();
    Client::new(connect, stream)
}

#[cfg(feature = "blocking")]
fn create_blocking_client(port: u16) -> blocking::Client {
    let stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .expect("Failed to open TCP connection to broker.");

    let connect = Connect::builder().client_id("test").keep_alive(5).build();
    blocking::Client::new(connect, stream)
}

// Connect a client to a broker.
// Then, subscribe to a topic and publish to that same topic.
// Verify that the client receives published message.
#[apply(test!)]
async fn test_subscribe_and_publish() {
    let broker = Broker::new();
    let (mut handle_a, task) = create_client(broker.port).await.spawn();
    let _handle = smol::spawn(task);

    // After connecting, the broker returns a CONNACK packet.
    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::ConnAck);

    handle_a
        .subscribe(TOPIC, tjiftjaf::QoS::AtMostOnceDelivery)
        .await
        .unwrap();
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

// Connect a client to a broker.
// Then, subscribe to a topic and publish to that same topic.
// Verify that the client receives published message.
#[cfg(feature = "blocking")]
#[test]
fn test_subscribe_and_publish_with_blocking_client() {
    let broker = Broker::new();
    let (mut handle_a, _task) = create_blocking_client(broker.port).spawn().unwrap();

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
    let client = create_client(server.local_addr().unwrap().port());

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

    handle_a.send(packet).await.unwrap();

    let _ = history.find(PacketType::PubRec).await;
    let _ = history.find(PacketType::PubRel).await;
    let _ = history.find(PacketType::PubComp).await;
}
