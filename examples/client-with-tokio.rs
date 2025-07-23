use async_compat::Compat;
use tjiftjaf::{Client, ClientHandle, Frame, Options};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    let stream = TcpStream::connect("test.mosquitto.org:1884")
        .await
        .expect("Failed connecting to MQTT broker.");

    let mut options = Options::default();
    options.client_id = Some("tjiftjaf".into());
    options.username = Some("ro".into());
    options.password = Some("readonly".into());
    let client = Client::new(options, Compat::new(stream));

    // Spawn the event loop that monitors the socket.
    // `handle` allows for sending and receiving MQTT packets.
    let (mut handle, task) = client.spawn();

    handle
        .subscribe("$SYS/broker/uptime")
        .await
        .expect("Failed to subscribe to topic.");

    tokio::select! {
        _ = print_packets(&mut handle)=> {}
        res = task => {
            res.expect("Event loop stopped!");
        }
    }
}

async fn print_packets(handle: &mut ClientHandle) {
    loop {
        let packet = handle
            .subscriptions()
            .await
            .expect("Failed to read packet.");

        let payload = String::from_utf8_lossy(packet.payload());
        println!("{} - {:?}", packet.topic(), payload);
    }
}
