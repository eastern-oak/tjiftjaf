#![no_main]
use libfuzzer_sys::fuzz_target;
use tjiftjaf::{Connect, Frame};

fuzz_target!(|connect_1: Connect| {
    // Verify this call doesn't panic.
    let bytes = connect_1.clone().into_bytes();
    let connect_2 = Connect::try_from(bytes.clone()).unwrap();

    // Verify that both packets are equal.
    assert_eq!(connect_1, connect_2);
    assert_eq!(&bytes, connect_2.as_bytes());

    // None of these calls should panic.
    _ = connect_1.flags();
    _ = connect_1.client_id();
    _ = connect_1.keep_alive();
    _ = connect_1.username();
    _ = connect_1.password();
    _ = connect_1.will();
});
