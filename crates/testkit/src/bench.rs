//! The harness that the benchmark files share: the on switch, the nearest-rank
//! percentiles, the budget of the build profile, and the artifact file that
//! the gate reads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};

use crate::stop_on;

/// The environment variable that turns the benchmarks on.
pub const BENCH_VAR: &str = "CADUS_BENCH";

/// The environment variable that moves the artifact directory.
pub const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

/// The multiplier of a time budget in a debug build.
///
/// The exact arithmetic of the checker runs about ten times slower without
/// optimization, and `crates/core/tests/answer_check.rs` carries the same
/// ten-times rule for the L2 budget. The gate runs the benchmarks in the
/// release profile, so the release literals are the numbers that bind.
#[cfg(debug_assertions)]
pub const BUDGET_FACTOR: u128 = 10;

/// The multiplier of a time budget in a release build: the literal binds.
#[cfg(not(debug_assertions))]
pub const BUDGET_FACTOR: u128 = 1;

/// The name of the build profile, for the artifact.
#[cfg(debug_assertions)]
pub const PROFILE: &str = "debug";

/// The name of the build profile, for the artifact.
#[cfg(not(debug_assertions))]
pub const PROFILE: &str = "release";

/// Whether this run asks for the benchmarks.
pub fn benchmarks_are_on() -> bool {
    std::env::var_os(BENCH_VAR).is_some()
}

/// The time budget of this build profile: the release literal, times
/// [`BUDGET_FACTOR`].
pub const fn budget(release_ns: u128) -> u128 {
    release_ns * BUDGET_FACTOR
}

/// The name of the build profile, for the artifact.
pub const fn profile() -> &'static str {
    PROFILE
}

/// The `percent` percentile of a sorted sample, by the nearest-rank rule.
///
/// The rank is `ceil(percent * n / 100)`, counted from one, and the function
/// reads the value at that rank. The arithmetic is integer arithmetic: no float
/// enters a reported number (D6).
pub fn percentile(sorted: &[u128], percent: u128) -> u128 {
    assert!(!sorted.is_empty(), "a percentile needs a sample");
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted[index.min(sorted.len() - 1)]
}

/// The p50, p95, p99, and maximum of a sample of nanosecond durations.
pub struct Percentiles {
    pub p50: u128,
    pub p95: u128,
    pub p99: u128,
    pub max: u128,
}

impl Percentiles {
    /// Read the percentiles of one sample. The function sorts its own copy.
    pub fn of(samples: &[u128]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            p99: percentile(&sorted, 99),
            max: percentile(&sorted, 100),
        }
    }

    /// The four numbers as one console phrase.
    pub fn phrase(&self) -> String {
        format!(
            "p50 {} ns, p95 {} ns, p99 {} ns, max {} ns",
            self.p50, self.p95, self.p99, self.max
        )
    }

    /// The four numbers as the JSON object of the artifact.
    pub fn json(&self) -> Json {
        json!({
            "p50_ns": self.p50,
            "p95_ns": self.p95,
            "p99_ns": self.p99,
            "max_ns": self.max,
        })
    }
}

/// The artifact directory: `CADUS_BENCH_DIR`, or `target/bench`.
pub fn artifact_dir() -> PathBuf {
    artifact_dir_of(std::env::var_os(ARTIFACT_DIR_VAR))
}

/// The artifact directory that the value of `CADUS_BENCH_DIR` names, or
/// `target/bench` of the workspace when the variable is absent.
pub fn artifact_dir_of(value: Option<OsString>) -> PathBuf {
    match value {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    }
}

/// Write one benchmark artifact into `dir`, print its path, and give the path
/// back.
pub fn write_artifact_to(dir: &Path, name: &str, body: &str) -> PathBuf {
    stop_on(
        format!("create {}", dir.display()),
        std::fs::create_dir_all(dir),
    );
    let path = dir.join(name);
    stop_on(
        format!("write {}", path.display()),
        std::fs::write(&path, body),
    );
    println!("artifact: {}", path.display());
    path
}

/// Write one benchmark artifact into the artifact directory, print its path,
/// and give the path back.
pub fn write_artifact(name: &str, body: &str) -> PathBuf {
    write_artifact_to(&artifact_dir(), name, body)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{
        BUDGET_FACTOR, PROFILE, Percentiles, artifact_dir_of, benchmarks_are_on, budget,
        percentile, profile, write_artifact,
    };

    /// The nearest-rank rule: the rank is `ceil(percent * n / 100)`, counted
    /// from one, and the maximum is the last sorted value.
    #[test]
    fn the_percentiles_follow_the_nearest_rank_rule() {
        let sorted: Vec<u128> = (1..=10).collect();
        assert_eq!(percentile(&sorted, 50), 5);
        assert_eq!(percentile(&sorted, 95), 10);
        assert_eq!(percentile(&sorted, 99), 10);
        assert_eq!(percentile(&sorted, 0), 1);
        let times = Percentiles::of(&[30, 10, 20]);
        assert_eq!(
            (times.p50, times.p95, times.p99, times.max),
            (20, 30, 30, 30)
        );
        assert_eq!(times.phrase(), "p50 20 ns, p95 30 ns, p99 30 ns, max 30 ns");
        assert_eq!(
            times.json(),
            json!({"p50_ns": 20, "p95_ns": 30, "p99_ns": 30, "max_ns": 30})
        );
    }

    #[test]
    #[should_panic(expected = "a percentile needs a sample")]
    fn an_empty_sample_has_no_percentile() {
        percentile(&[], 50);
    }

    /// The gate reads the environment of the run: the clock is off, the debug
    /// budget is ten times wider, and the profile names the build.
    #[test]
    fn the_budget_and_the_profile_follow_the_build() {
        assert!(!benchmarks_are_on(), "the coverage run sets no CADUS_BENCH");
        assert_eq!(budget(100), 100 * BUDGET_FACTOR);
        assert_eq!(profile(), PROFILE);
        assert_eq!(BUDGET_FACTOR == 10, PROFILE == "debug");
    }

    /// `CADUS_BENCH_DIR` names the directory; an absent variable gives
    /// `target/bench` of the workspace.
    #[test]
    fn the_artifact_directory_follows_the_variable() {
        assert_eq!(
            artifact_dir_of(Some("/somewhere/else".into())),
            Path::new("/somewhere/else")
        );
        assert!(artifact_dir_of(None).ends_with("target/bench"));
    }

    /// The artifact lands in the artifact directory under its name, with the
    /// body as given.
    #[test]
    fn the_artifact_lands_in_the_directory() {
        let path = write_artifact("testkit-check.json", "{\"ok\": true}\n");
        assert!(path.ends_with("testkit-check.json"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"ok\": true}\n");
    }
}
