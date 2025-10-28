use async_net::TcpStream;
use futures_lite::FutureExt;
use tjiftjaf::{Client, Frame, Options, QoS};

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    smol::block_on(async {
        let stream = TcpStream::connect("test.mosquitto.org:1884")
            .await
            .expect("Failed connecting to MQTT broker.");

        let mut options = Options::default();
        options.client_id = Some("tjiftjaf".into());
        options.username = Some("ro".into());
        options.password = Some("readonly".into());
        let client = Client::new(options, stream);

        // Spawn the event loop that monitors the socket.
        // `handle` allows for sending and receiving MQTT packets.
        let (mut handle, task) = client.spawn();

        handle
            .subscribe("$SYS/broker/uptime", QoS::AtMostOnceDelivery)
            .await
            .expect("Failed to subscribe to topic.");

        _ = task
            .race(async {
                loop {
                    let packet = handle
                        .subscriptions()
                        .await
                        .expect("Failed to read packet.");

                    let payload = String::from_utf8_lossy(packet.payload());
                    println!("{} - {:?}", packet.topic(), payload);
                }
            })
            .await;
    })
}
