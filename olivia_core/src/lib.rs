#![no_std]
mod attestation;
mod schnorr;
mod event;
mod outcome;
mod entity;
mod macros;
#[doc(hidden)]
pub mod hex;
pub mod http;
pub use attestation::*;
pub use schnorr::*;
pub use event::*;
pub use outcome::*;
pub use entity::*;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;





