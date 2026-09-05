//! The test instruments that the crates of the workspace share: the benchmark
//! harness, the fake HTTP endpoint, the DSN helper of a process test, the R4
//! purity guards, and the fixture texts.
//!
//! Every crate lists this crate under `[dev-dependencies]` only, so no line of
//! it enters a runtime binary. A helper that fails stops the test, so the crate
//! panics where a production crate returns an error.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Display;

pub mod bench;
pub mod fixtures;
pub mod http;
pub mod process;
pub mod purity;
pub mod sources;

/// The value of `result`, or a stop of the test with `what` and the error.
///
/// A helper that cannot build its fixture stops the test in place of a false
/// result, so every fallible step of this crate ends here.
pub fn stop_on<T, E: Display>(what: impl Display, result: Result<T, E>) -> T {
    result.unwrap_or_else(|err| panic!("{what}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::stop_on;

    #[test]
    fn stop_on_gives_the_value_back() {
        assert_eq!(stop_on("a step", Ok::<u8, &str>(7)), 7);
    }

    #[test]
    #[should_panic(expected = "a step: broke")]
    fn stop_on_stops_the_test_with_the_step_and_the_error() {
        stop_on("a step", Err::<u8, &str>("broke"));
    }
}
