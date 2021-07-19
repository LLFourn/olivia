#![no_std]
mod announcement;
mod attestation;
mod descriptor;
mod event;
mod group;
pub mod http;
mod macros;
mod node;
mod oracle_info;
mod outcome;
mod path;

pub use announcement::*;
pub use attestation::*;
pub use descriptor::*;
pub use event::*;
pub use group::*;
pub use node::*;
pub use oracle_info::*;
pub use outcome::*;
pub use path::*;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

pub use chrono;
#[cfg(feature = "postgres-types")]
pub use postgres_types;

pub trait PrefixPath {
    fn prefix_path(self, path: PathRef<'_>) -> Self;
    fn strip_prefix_path(self, path: PathRef<'_>) -> Self;
}

#[macro_export]
macro_rules! path {
    ($path:literal) => {
        $crate::Path::from_str($path).unwrap().as_path_ref()
    };
}
