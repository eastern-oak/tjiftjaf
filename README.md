<p align="center">
  <img src="https://raw.githubusercontent.com/eastern-oak/tjiftjaf/master/logo.svg">
</p>

_tjiftjaf_ is a Rust library implementing MQTT 3.1.1.

# Features
**MQTT Client**

The crate provides a [blocking `Client`](https://docs.rs/tjiftjaf/latest/tjiftjaf/blocking/index.html)
and an [asynchronous `Client`](https://docs.rs/tjiftjaf/latest/tjiftjaf/aio/index.html).

The latter does not require a specific runtime executor.

Take a look at the examples:
* [examples/client_with_smol.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/client_with_smol.rs) uses the executor [smol](https://docs.rs/smol/latest/smol/index.html)
* [examples/client_with_tokio.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/client_with_tokio.rs) uses the executor [tokio](https://docs.rs/tokio/latest/tokio/index.html)
* [examples/blocking_client.rs](https://github.com/eastern-oak/tjiftjaf/blob/master/examples/blocking_client.rs) does _not_ use async.

## Do not use this crate

I created this project to learn more about MQTT, [fuzzing](https://rust-fuzz.github.io/book/introduction.html),
[sans-io](https://www.firezone.dev/blog/sans-io) and [zero-copy](https://rkyv.org/zero-copy-deserialization.html).

Do not rely on this crate for your own projects. It's unstable.
Consider using [rumqttc](https://docs.rs/crate/rumqttc/latest) instead.

## Fuzzer

Portions of the code are verified using fuzzing. Make sure to install
[`cargo fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html) before
running the examples.

```shell
cargo +nightly fuzz run subscribe
```

List all fuzz targets using:

```shell
cargo +nightly fuzz list
```

## License
This project is licensed under the [Mozilla Public License](https://github.com/eastern-oak/tjiftjaf/blob/master/LICENSE).
