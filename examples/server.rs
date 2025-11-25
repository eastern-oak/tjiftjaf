use async_net::TcpListener;
use std::env;
use tjiftjaf::aio::server::Server;

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let broker = env::args().nth(1).unwrap_or(String::from("127.0.0.1:1883"));

    smol::block_on(async {
        let listener = TcpListener::bind(broker)
            .await
            .expect("Failed to boot to MQTT server.");

        let server = Server::new(listener);
        server.run().await
    })
}
