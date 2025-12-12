use async_net::TcpStream;
use bytes::Bytes;
use futures_lite::FutureExt;
use log::info;
use std::env;
use tjiftjaf::{
    aio::{Client, Emit},
    packet_identifier, publish, subscribe, Connect,
};

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    let broker = env::args()
        .nth(1)
        .unwrap_or(String::from("test.mosquitto.org:1884"));

    smol::block_on(async {
        let stream = TcpStream::connect(broker)
            .await
            .expect("Failed connecting to MQTT broker.");

        let connect = Connect::builder()
            .client_id("tjiftjaf")
            .username("ro")
            .password("readonly")
            .build();
        let client = Client::new(connect, stream);

        // Spawn the event loop that monitors the socket.
        // `handle` allows for sending and receiving MQTT packets.
        let (mut handle, task) = client.spawn();

        subscribe("$SYS/broker/uptime")
            .emit(&handle)
            .await
            .expect("Failed to subscribe to topic.");

        let random_topic = packet_identifier().to_string();
        subscribe(&random_topic)
            .emit(&handle)
            .await
            .expect("Failed to subscribe to topic.");

        let mut n = 0;
        _ = task
            .race(async {
                loop {
                    let packet = handle
                        .subscriptions()
                        .await
                        .expect("Failed to read packet.");

                    n += 1;

                    let payload = String::from_utf8_lossy(packet.payload());
                    info!("{} - {:?}", packet.topic(), payload);
                    if packet.topic() == "$SYS/broker/uptime" {
                        publish(
                            &random_topic,
                            Bytes::copy_from_slice(format!("{n} packets received").as_bytes()),
                        )
                        .emit(&handle)
                        .await
                        .unwrap()
                    }
                }
            })
            .await;
    })
}
