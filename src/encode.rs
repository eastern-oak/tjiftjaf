use bytes::{BufMut, Bytes, BytesMut};

pub fn utf8(value: String) -> Bytes {
    let mut bytes = BytesMut::with_capacity(value.len() + 2);
    // TODO: Check for maximum lenght of string.
    bytes.put_u16(value.len() as u16);
    bytes.put(value.as_bytes());

    bytes.freeze()
}

pub fn bytes(value: &[u8]) -> Bytes {
    let mut bytes = BytesMut::with_capacity(value.len() + 2);
    // TODO: Check for maximum lenght of string.
    bytes.put_u16(value.len() as u16);
    bytes.put_slice(value);

    bytes.freeze()
}
pub fn remaining_length(length: usize) -> Bytes {
    let mut length = length;
    let mut bytes = BytesMut::with_capacity(1);

    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;

        if length > 0 {
            byte |= 128;
        }
        bytes.put_u8(byte);

        if length == 0 {
            break;
        }
    }
    assert!(bytes.len() <= 4);
    bytes.freeze()
}
