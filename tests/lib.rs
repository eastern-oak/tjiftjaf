use async_net::TcpStream;
use broker::Broker;
use bytes::{Bytes, BytesMut};
use pretty_assertions::assert_eq;
use tjiftjaf::{Client, Frame, Options, Packet, PacketType};
mod broker;
use macro_rules_attribute::apply;
use smol_macros::test;

const TOPIC: &'static str = "topic";

async fn create_client(broker: &Broker) -> Client<TcpStream> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", broker.port))
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
    let (mut handle_a, task) = create_client(&broker).await.spawn();
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

#[apply(test!)]
async fn test_large_packets() {
    let broker = Broker::new();
    let (mut handle_a, task) = create_client(&broker).await.spawn();
    let _handle = smol::spawn(task);

    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::ConnAck);

    handle_a.subscribe(TOPIC).await.unwrap();
    let packet = handle_a.any_packet().await.unwrap();
    assert_eq!(packet.packet_type(), PacketType::SubAck);

    handle_a
        .publish(TOPIC, BytesMut::zeroed(50_000).freeze())
        .await
        .unwrap();
}
