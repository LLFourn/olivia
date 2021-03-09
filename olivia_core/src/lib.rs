#![no_std]
mod announcement;
mod attestation;
mod descriptor;
mod entity;
mod event;
#[doc(hidden)]
pub mod hex;
pub mod http;
mod macros;
mod outcome;
mod schnorr;
mod oracle_info;

pub use announcement::*;
pub use attestation::*;
pub use descriptor::*;
pub use entity::*;
pub use event::*;
pub use outcome::*;
pub use schnorr::*;
pub use oracle_info::*;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;
