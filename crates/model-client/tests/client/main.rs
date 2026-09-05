//! The one test binary of the client.
//!
//! One binary holds every test, so one copy of each async body of the crate
//! carries the coverage of all the tests (`cargo llvm-cov` reports the
//! best-covered copy of a function, not the union of its copies).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../common/mod.rs"]
mod common;

mod body;
mod record;
mod retry;
mod transport;
