pub fn utf8(value: String) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len() + 2);

    // TODO: Check for maximum length of string.
    bytes.extend_from_slice(&((value.len() as u16).to_be_bytes()));
    bytes.append(&mut value.into_bytes());
    bytes
}

// TODO: Consider taking `Vec<u8>` to make clear that
// function clones value.
pub fn bytes(value: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len() + 2);
    // TODO: Check for maximum length of string.
    bytes.extend_from_slice(&((value.len() as u16).to_be_bytes()));
    bytes.extend_from_slice(value);
    bytes
}

pub fn remaining_length(length: usize) -> Vec<u8> {
    // TODO: proper validation and error handling.
    assert!(length <= 268_435_455);

    let mut length = length;
    let mut bytes = Vec::with_capacity(1);

    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;

        if length > 0 {
            byte |= 128;
        }
        bytes.push(byte);

        if length == 0 {
            break;
        }
    }
    assert!(bytes.len() <= 4);
    bytes
}
