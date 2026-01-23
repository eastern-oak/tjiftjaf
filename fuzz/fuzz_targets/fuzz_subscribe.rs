#![no_main]
use libfuzzer_sys::fuzz_target;
use tjiftjaf::{packet::subscribe::Builder, Frame, Subscribe};

fuzz_target!(|data: Builder| {
    // Verify this call doesn't panic.
    let subscribe_1 = data.build();
    let bytes = subscribe_1.clone().into_bytes();
    let subscribe_2 = Subscribe::try_from(bytes.clone()).unwrap();

    // Verify that both packets are equal.
    assert_eq!(subscribe_1, subscribe_2);
    assert_eq!(&bytes, subscribe_2.as_bytes());

    // All these calls expect a correct packet. If packet is incorrect,
    // the calls cause panic.
    subscribe_1.packet_identifier();
    let topics = subscribe_1.topics();
    for _ in topics {}
});
