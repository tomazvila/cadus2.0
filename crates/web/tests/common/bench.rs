//! The plumbing the benchmark files of this crate share: the on switch, the
//! nearest-rank percentiles, the budget of the build profile, and the
//! artifact file the gate reads.

use std::path::{Path, PathBuf};

/// The environment variable that turns the benchmarks on.
pub const BENCH_VAR: &str = "CADUS_BENCH";

/// The environment variable that moves the artifact directory.
pub const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

/// The debug-profile multiplier of a time budget.
///
/// The exact arithmetic runs about ten times slower without optimization,
/// which is the rule `crates/core/tests/bench_l1.rs` and
/// `crates/core/tests/answer_check.rs` already carry.
pub const DEBUG_SLOWDOWN: u128 = 10;

/// Whether this run asks for the benchmarks.
pub fn benchmarks_are_on() -> bool {
    std::env::var_os(BENCH_VAR).is_some()
}

/// The `percent` percentile of a sorted sample, by the nearest-rank rule: the
/// value at rank `ceil(percent * n / 100)`, counted from one. An empty sample
/// gives zero. The arithmetic is integer arithmetic (D6).
pub fn percentile(sorted: &[u128], percent: u128) -> u128 {
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).clamp(1, count.max(1));
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted.get(index).copied().unwrap_or(0)
}

/// The p50, p95, p99 and maximum of a sample of nanosecond durations.
pub struct Percentiles {
    pub p50: u128,
    pub p95: u128,
    pub p99: u128,
    pub max: u128,
}

impl Percentiles {
    /// Read the four numbers of one sample. The function sorts its own copy.
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
    pub fn json(&self) -> String {
        format!(
            "{{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \"max_ns\": {}}}",
            self.p50, self.p95, self.p99, self.max
        )
    }
}

/// The name of the build profile, for the artifact.
pub fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// The time budget of this build profile: the release literal, or ten times
/// it in a debug build.
pub fn budget(release_ns: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ns * DEBUG_SLOWDOWN
    } else {
        release_ns
    }
}

/// The directory the artifacts go to: `CADUS_BENCH_DIR`, or `target/bench`.
pub fn artifact_dir() -> PathBuf {
    std::env::var_os(ARTIFACT_DIR_VAR).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
        PathBuf::from,
    )
}

/// Write one artifact file, print its path, and give the path back.
pub fn write_artifact(name: &str, body: &str) -> PathBuf {
    let dir = artifact_dir();
    std::fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    println!("artifact: {}", path.display());
    path
}
