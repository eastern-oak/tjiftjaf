/// A namespace where messages are published to.
/// A `Topic` must be valid utf-8 with a minimum length of 1 bytes.
/// It can't contain the wildcards `#` and `+`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Topic<'a>(&'a str);

impl<'a> Topic<'a> {
    pub fn new(value: &'a str) -> Result<Self, ValueError> {
        if value.is_empty() {
            return Err(ValueError::new("Topic", "must have at least one byte"));
        }

        verify_utf8(value).unwrap();
        if value.contains('#') || value.contains('+') {
            return Err(ValueError::new(
                "Topic",
                "contains the wildcard '#' and/or a '+'",
            ));
        }

        Ok(Self(value))
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        self.0
    }
}

impl<'a> PartialEq<&str> for Topic<'a> {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl AsRef<str> for Topic<'_> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for Topic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Filter must be at least 1 char long.
// Must be valid utf-8.
// Wildcard '#' is the last char of a filter.
// Wildcard '#' is preceded by nothing or a '/'
// Wildcard '+' must be preceded by nothing or a '/'
// pub struct Filter(String);

#[derive(Debug)]
pub struct ValueError {
    field: String,
    problem: String,
}

impl ValueError {
    pub fn new(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            problem: problem.into(),
        }
    }
}

impl std::error::Error for ValueError {}

impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not  valid: {}", self.field, self.problem)
    }
}

pub fn verify_utf8(value: &str) -> Result<(), EncodingError> {
    // 1.5.3 [..] you cannot use a string that would encode to more than 65_535 bytes.
    if value.len() > 65_535 {
        return Err(EncodingError::TooLong);
    }

    // [MQTT-1.5.3-2] A UTF-8 encoded string MUST NOT include an encoding of the null character U+0000.
    if value.contains('\0') {
        return Err(EncodingError::IllegalValue);
    }

    Ok(())
}

/// Encode a string as bytes.
///
/// The first 2 bytes encode the strings length, followed by
/// the string.
pub fn utf8(value: String) -> Result<Vec<u8>, EncodingError> {
    verify_utf8(&value)?;

    let mut bytes = Vec::with_capacity(value.len() + 2);

    bytes.extend_from_slice(&((value.len() as u16).to_be_bytes()));
    bytes.append(&mut value.into_bytes());
    Ok(bytes)
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

#[derive(Debug, Copy, Clone)]
pub enum EncodingError {
    // Value exceeds length
    TooLong,

    // Illegal value.
    IllegalValue,
}
