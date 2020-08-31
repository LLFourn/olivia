#![feature(async_closure)]
#![allow(non_snake_case)]
pub mod db;

pub mod oracle;
pub mod seed;
pub use crate::oracle::Oracle;

pub mod cli;
pub mod config;
pub mod core;
pub mod curve;
pub mod keychain;
pub mod log;
mod macros;
pub mod rest_api;
pub mod sources;
mod util;

#[macro_use]
extern crate slog;

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;

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
