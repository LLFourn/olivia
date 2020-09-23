#![feature(async_closure)]
#![allow(non_snake_case)]
pub mod db;

pub mod oracle;
pub mod seed;
pub use crate::oracle::Oracle;

pub mod cli;
pub mod config;
pub mod curve;
pub mod keychain;
pub mod log;
pub mod rest_api;
pub mod sources;
mod util;

pub use olivia_core as core;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate serde_derive;
