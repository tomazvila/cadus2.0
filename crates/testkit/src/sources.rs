//! The R4 source guard: no handler opens a socket or starts a process.
//!
//! The dependency guards of [`crate::purity`] see dependencies only, and
//! `std::net` and `std::process` are dependencies of nothing, so a body that
//! opens a socket by hand passes both. This scan reads the SOURCE of the tiers.

use std::path::{Path, PathBuf};

use crate::stop_on;

/// Tokens that must not appear in the source of `cadus-web` and `cadus-worker`.
///
/// A request handler does local CPU work and DB I/O only (R4). Each token is
/// one way out of the process:
///
/// - `reqwest` is an outbound HTTP client. The name also catches a `use` line
///   that a manifest edit smuggled in.
/// - `TcpStream::connect` and `UdpSocket` open a raw socket.
/// - `hyper::Client` and `hyper_util::client` build an HTTP client from the
///   server-side crates that `axum` already pulls in.
/// - `std::process::Command` and `Command::new` start a child process, which
///   runs any of the above outside the process.
pub const FORBIDDEN_SOURCE_TOKENS: [&str; 7] = [
    "reqwest",
    "TcpStream::connect",
    "UdpSocket",
    "hyper::Client",
    "hyper_util::client",
    "std::process::Command",
    "Command::new",
];

/// The source directories of the two tiers, relative to the repository root.
///
/// The scan reads both tiers from both purity tests, so a token in either crate
/// fails both tests.
pub const TIER_SOURCE_DIRS: [&str; 2] = ["crates/web/src", "crates/worker/src"];

/// The four source files that prove the walk read both tiers.
pub const TIER_ROOT_FILES: [&str; 4] = [
    "crates/web/src/lib.rs",
    "crates/web/src/bin/cadus-web/main.rs",
    "crates/worker/src/lib.rs",
    "crates/worker/src/bin/cadus-worker.rs",
];

/// The root of the repository.
///
/// `CARGO_MANIFEST_DIR` is `<root>/crates/testkit`, so the root is two levels
/// up.
pub fn repo_root() -> PathBuf {
    root_of(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// The grandparent of a crate directory, which must hold the workspace
/// `Cargo.toml`.
pub fn root_of(manifest_dir: &Path) -> PathBuf {
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("the manifest directory has a grandparent")
        .to_path_buf();
    assert!(
        root.join("Cargo.toml").is_file(),
        "`{}` is not the repository root",
        root.display()
    );
    root
}

/// Collect every `.rs` file under `dir`, at any depth.
///
/// The scan reads production source only. A test file lives under `tests/`, and
/// this walk starts at `src`, so it never reads one. That is the whole rule:
/// inside `src` the scan reads every line, `#[cfg(test)]` blocks included.
pub fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let what = format!("read_dir {}", current.display());
        let entries = stop_on(&what, std::fs::read_dir(&current));
        for entry in entries {
            let path = stop_on(&what, entry).path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The lines of `file` that hold a token of `tokens`, as `path:line: token: text`.
fn hits_in(root: &Path, file: &Path, tokens: &[&str]) -> Vec<String> {
    let text = stop_on(
        format!("read {}", file.display()),
        std::fs::read_to_string(file),
    );
    let shown = file
        .strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();
    let mut hits = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for token in tokens {
            if line.contains(token) {
                hits.push(format!("{shown}:{}: {token}: {}", index + 1, line.trim()));
            }
        }
    }
    hits
}

/// R4 at the source level: no `.rs` file under `dirs` holds a token of
/// `tokens`, and the walk read every file of `expected`.
///
/// Every path is relative to the repository root. `expected` proves the walk
/// read the known source files; without it, a walk that returns nothing makes
/// the token check pass on an empty set.
pub fn check_sources(dirs: &[&str], tokens: &[&str], expected: &[&str]) {
    let root = repo_root();
    let mut scanned: Vec<PathBuf> = Vec::new();
    let mut hits: Vec<String> = Vec::new();

    for dir in dirs {
        let files = rust_sources(&root.join(dir));
        assert!(
            !files.is_empty(),
            "the source scan found no `.rs` file under `{dir}`; the walk is broken"
        );
        for file in files {
            hits.extend(hits_in(&root, &file, tokens));
            scanned.push(file);
        }
    }

    for name in expected {
        let wanted = root.join(name);
        assert!(
            scanned.contains(&wanted),
            "the source scan missed `{name}`; it read {} files",
            scanned.len()
        );
    }

    assert!(
        hits.is_empty(),
        "R4: a handler does local CPU work and DB I/O only; these lines leave the process:\n{}",
        hits.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::{
        FORBIDDEN_SOURCE_TOKENS, TIER_ROOT_FILES, TIER_SOURCE_DIRS, check_sources, repo_root,
        root_of, rust_sources,
    };

    #[test]
    #[should_panic(expected = "is not the repository root")]
    fn a_directory_without_a_workspace_manifest_is_no_root() {
        root_of(&repo_root().join("crates/testkit/src"));
    }

    /// The walk reads this crate: its manifest is not a source, its seven
    /// `.rs` files are.
    #[test]
    fn the_walk_reads_the_rust_sources_of_a_tree() {
        let found = rust_sources(&repo_root().join("crates/testkit"));
        assert_eq!(found.len(), 7, "{found:?}");
        assert!(found[0].ends_with("src/bench.rs"));
    }

    /// The two tiers pass the scan, and this crate passes it with a token it
    /// never holds: the unit separator, which no source file carries.
    #[test]
    fn the_tiers_and_this_crate_hold_no_forbidden_token() {
        check_sources(
            &TIER_SOURCE_DIRS,
            &FORBIDDEN_SOURCE_TOKENS,
            &TIER_ROOT_FILES,
        );
        check_sources(
            &["crates/testkit"],
            &["\u{1f}"],
            &["crates/testkit/src/lib.rs"],
        );
    }

    #[test]
    #[should_panic(expected = "found no `.rs` file under `scripts/quality`")]
    fn a_tree_without_a_source_stops_the_scan() {
        check_sources(&["scripts/quality"], &[], &[]);
    }

    #[test]
    #[should_panic(expected = "read_dir")]
    fn a_tree_that_is_absent_stops_the_scan() {
        check_sources(&["crates/no-such-crate"], &[], &[]);
    }

    #[test]
    #[should_panic(expected = "the source scan missed `crates/testkit/src/none.rs`")]
    fn a_file_the_walk_did_not_read_stops_the_scan() {
        check_sources(
            &["crates/testkit/src"],
            &[],
            &["crates/testkit/src/none.rs"],
        );
    }

    #[test]
    #[should_panic(expected = "these lines leave the process:\ncrates/testkit/src/sources.rs")]
    fn a_line_with_a_token_stops_the_scan() {
        check_sources(&["crates/testkit/src"], &["pub fn check_sources"], &[]);
    }
}
