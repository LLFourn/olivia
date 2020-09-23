use alloc::string::String;
use core::fmt::Write;

#[doc(hidden)]
pub fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::new();
    for byte in bytes {
        write!(hex, "{:02x}", byte).unwrap()
    }
    hex
}

#[derive(Debug, Clone, PartialEq)]
pub enum HexError {
    /// The string was not a valid hex string.
    InvalidHex,
    /// The string was not the right length for the target type.
    InvalidLength,
    /// The bytes did not encode a valid value for the target type.
    InvalidEncoding,
}

#[doc(hidden)]
pub fn hex_val(c: u8) -> Result<u8, HexError> {
    match c {
        b'A'..=b'F' => Ok(c - b'A' + 10),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'0'..=b'9' => Ok(c - b'0'),
        _ => Err(HexError::InvalidHex),
    }
}
