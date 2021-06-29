/////////////////////////////////////
// THIS WAS PASTED FROM SECP25kFUN //
/////////////////////////////////////

#[derive(Debug, Clone, PartialEq)]
pub enum HexError {
    /// The string was not a valid hex string.
    InvalidHex,
    /// The string was not the right length for the target type.
    InvalidLength,
    /// The bytes did not encode a valid value for the target type.
    InvalidEncoding,
}

impl core::fmt::Display for HexError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        use HexError::*;
        match self {
            InvalidHex => write!(f, "invalid hex string"),
            InvalidLength => write!(f, "hex string had an invalid (odd) length"),
            InvalidEncoding => write!(f, "hex value did not encode the expected type"),
        }
    }
}

impl std::error::Error for HexError {}

#[doc(hidden)]
pub fn hex_val(c: u8) -> Result<u8, HexError> {
    match c {
        b'A'..=b'F' => Ok(c - b'A' + 10),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'0'..=b'9' => Ok(c - b'0'),
        _ => Err(HexError::InvalidHex),
    }
}
