# Tjiftjaf

_tjiftjaf_ is a Rust library implementing MQTT 3.1.1.

It features:

* encoding and decoding support for all 14 control packets.
* a sans-io `Client` that supports both blocking and async paradigms. See [examples/](examples/) for more information.

# Do not use this crate

I created this project to learn more about MQTT, [fuzzing](https://rust-fuzz.github.io/book/introduction.html),
[sans-io](https://www.firezone.dev/blog/sans-io) and [zero-copy](https://rkyv.org/zero-copy-deserialization.html).

Do not rely on this crate for your own projects. It's unstable.
Consider using [rumqttc](https://docs.rs/crate/rumqttc/latest) instead.

# License
This project is licensed under the [Mozilla Public License](LICENSE).
