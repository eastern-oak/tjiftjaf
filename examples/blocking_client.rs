/// Run with `cargo run --example blocking_client --feature=blocking`
use log::info;
use std::env;
use std::net::TcpStream;
use tjiftjaf::{
    blocking::{Client, Emit},
    packet_identifier, publish, subscribe, Connect,
};

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let broker = env::args()
        .nth(1)
        .unwrap_or(String::from("test.mosquitto.org:1884"));
    let stream = TcpStream::connect(broker).unwrap();

    let connect = Connect::builder()
        .client_id("tjiftjaf")
        .username("ro")
        .password("readonly")
        .build();
    let client = Client::new(connect, stream);

    // Spawn the event loop that monitors the socket.
    // `handle` allows for sending and receiving MQTT packets.
    let (mut handle, _task) = client.spawn().unwrap();

    subscribe("$SYS/broker/uptime")
        .emit(&handle)
        .expect("Failed to subscribe to topic.");

    subscribe("$SYS/broker/load/publish/sent")
        .emit(&handle)
        .expect("Failed to subscribe to topic.");

    let random_topic = packet_identifier().to_string();
    subscribe(&random_topic)
        .emit(&handle)
        .expect("Failed to subscribe to topic.");

    let mut n = 0;
    loop {
        let packet = handle.publication().expect("Failed to read packet.");

        n += 1;

        let payload = String::from_utf8_lossy(packet.payload());
        info!("{} - {:?}", packet.topic(), payload);
        if packet.topic() == "$SYS/broker/uptime" {
            publish(&random_topic, format!("{n} packets received"))
                .emit(&handle)
                .unwrap();
        }
    }
}
