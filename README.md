# Tjiftjaf

Tjiftjaf is a MQTT 3.1.1 client written in Rust.

Below, you find an example how `tjiftjaf` integrates with the
standard library. See [examples/](examples/) to learn how you can use `tjiftjaf`
with [tokio](examples/client-with-tokio.rs) or [smol](examples/client-with-smol.rs).
You can even use it in the web browser!

## Quick start

```bash
$ cargo add tjiftjaf
```

```rust
TODO
```

Most users interact with a few types only:

1. The `Client` interacting with network socket.
2. An application uses a `Handle` to receive `Packet`s and send `Packet`s to the broker.
3. The `Subscribe` packet to express interest in a certain topic.
4. The `Publish` packet to publish information to a topic and to receive data that has been published.


## Features

| MQTT Message Type | Status |
|------------------|--------|
| CONNECT | 🔶 | Will not yet supported |
| CONNACK | ✅ |
| PUBLISH | 🔶 | Configuring Quality of Service not yet supported|
| PUBACK | ✅|
| PUBREC | ❌ |
| PUBREL | ❌ |
| PUBCOMP | ❌ |
| SUBSCRIBE | 🔶 | Subscribing to multiple topics is not yet supported |
| SUBACK | ✅ |
| UNSUBSCRIBE | ❌ |
| UNSUBACK | ❌ |
| PINGREQ | ✅ |
| PINGRESP | ✅ |
| DISCONNECT | ❌ |



## Goals

* Offer an ergonomic API that is hard to misuse.
* Support both asynchronous and blocking environments
* Keep the number of dependencies minimal

Efficient use of memory and compute is a secondary goal.

## Design

The `MqttBinding` is the heart of `tjiftjaf`. The `MqttBinding` is implemented using according to sans-IO.
To learn more about this design pattern,  read Firezone's post
["Sans-IO: The secret to effective Rust for network services"](https://www.firezone.dev/blog/sans-io].

The binding includes two state machine
One state machine keeps track of the connection status. An MQTT client is only allowed to
send MQTT packets _after_ the it receives a positive `ConnAck` packet from the server.

The other state machines decodes bytes as `Packet`s. It tracks how many more bytes
are required to construct a `Packet`. `MqttBinding.get_input_buffer()` hands out buffers
with a lenght that's dictated by that state machine.

The `MqttBinding` doesn't do anything by itself, it must be driven by an event loop.
The event loop has three tasks:

1. Call `MqttBinding.get_input_buffer()`, fill this buffer with bytes read from the network connection, and
feed these bytes to `MqttBinding.try_decode()`.
2. Call `MqttBinding.poll_transmits()` and write these bytes to the network connection.
3. Wait for `Packet`s from the application and forward those to `MqttBinding.send()`.

`Client.run()` implements such an event loop.
