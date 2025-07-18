use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Mutex,
    thread,
    time::Duration,
};

use rumqttd::{Config, ConnectionSettings, ServerSettings};

// We keep `USED_PORTS` behind a mutex so in tests that runs in parallel starting an `MqttServer`
// won't try to reuse a port
static USED_PORTS: Mutex<Vec<u16>> = Mutex::new(Vec::new());

/// Returns the next free port from the OS perspective
fn next_free_port() -> u16 {
    let mut handle = USED_PORTS.lock().unwrap();

    let port = loop {
        // Try to bind to port 0 so the OS assigns a free port for us
        let addr = "localhost:0";
        let try_socket = TcpListener::bind(addr);
        let listener = try_socket.expect("Failed to bind");
        let port = listener.local_addr().unwrap().port();
        if !handle.contains(&port) {
            break port;
        }
    };
    handle.push(port);
    port
}

fn get_broker_config(port: u16) -> Config {
    let listen = format!("127.0.0.1:{port}").parse().unwrap();
    let v4_config = HashMap::from([(
        "v4".to_string(),
        ServerSettings {
            name: "v4".to_string(),
            tls: None,
            listen,
            next_connection_delay_ms: 1,
            connections: ConnectionSettings {
                connection_timeout_ms: 60000,
                max_payload_size: 20480,
                max_inflight_count: 100,
                auth: None,
                external_auth: None,
                dynamic_filters: true,
            },
        },
    )]);

    Config {
        router: rumqttd::RouterConfig {
            max_connections: 10010,
            max_outgoing_packet_count: 200,
            max_segment_size: 104857600,
            max_segment_count: 10,
            ..Default::default()
        },
        v4: Some(v4_config),
        ..Default::default()
    }
}

// Waits until a connection can be opened to `port`
fn wait_server_listening(port: u16) {
    while TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(1),
    )
    .is_err()
    {
        thread::sleep(Duration::from_millis(10))
    }
}
/// Mqtt broker used for tests, it spawns a background server in a random free port.
pub struct Broker {
    pub port: u16,
}

impl Drop for Broker {
    fn drop(&mut self) {
        // TODO: implement killing the server and releasing ports once https://github.com/bytebeamio/rumqtt/issues/770 is done
    }
}

impl Broker {
    /// Start a new MQTT broker that accepts connection on a random port.
    pub fn new() -> Self {
        let port = next_free_port();
        let config = get_broker_config(port);
        let mut broker = rumqttd::Broker::new(config);
        let _ = thread::spawn(move || {
            broker.start().expect("Failed to start the MQTT broker.");
        });

        // Since the server is running in a thread and we don't have control over when is ready
        // we wait for the port to be open, it shouldn't take more than a few milliseconds
        wait_server_listening(port);

        Self { port }
    }
}
