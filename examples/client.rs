use async_net::TcpStream;
use futures_lite::FutureExt;
use log::info;
use std::env;
use tjiftjaf::{QoS, asynchronous::Client, packet_identifier, packet_v2::connect::Connect};

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

        handle
            .subscribe("$SYS/broker/uptime", QoS::AtMostOnceDelivery)
            .await
            .expect("Failed to subscribe to topic.");

        let random_topic = packet_identifier().to_string();
        handle
            .subscribe(&random_topic, QoS::AtMostOnceDelivery)
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
                        handle
                            .publish(&random_topic, format!("{n} packets received").into())
                            .await
                            .unwrap();
                    }
                }
            })
            .await;
    })
}
