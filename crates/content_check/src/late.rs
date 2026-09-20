//! The subcommands of lane B5b: `report`, `row`, `diff`, `dump-kp`, `selftest`.
//!
//! This file is a stub. Lane B5b replaces the body and adds its modules here
//! with `mod` lines, so that `main.rs` does not change.

use serde_json::Value;

/// Run one late subcommand. `None` tells `main.rs` that the subcommand is not
/// in this build.
pub fn run(subcommand: &str, args: &[String]) -> Option<(Value, u8)> {
    let _ = (subcommand, args);
    None
}
