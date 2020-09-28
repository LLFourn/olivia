#![no_std]
mod attestation;
mod entity;
mod event;
#[doc(hidden)]
pub mod hex;
pub mod http;
mod macros;
mod outcome;
mod schnorr;
pub use attestation::*;
pub use entity::*;
pub use event::*;
pub use outcome::*;
pub use schnorr::*;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;





