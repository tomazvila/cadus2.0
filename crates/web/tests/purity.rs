//! R4 — an HTTP client or a model SDK on the request path is a reviewable diff.
//!
//! `cadus-web` answers requests. A model call belongs to the worker, so the
//! request path carries no outbound HTTP client and no model SDK.
//!
//! Round-1 finding #22 asked for this guard. The first version read the literal
//! `[dependencies]` table of one manifest, and round-4 finding #6 showed two
//! ways past it: a client under `[target.'cfg(unix)'.dependencies]` of this
//! crate, and a client in `cadus-store`, which every handler imports. Both link
//! into the binary.
//!
//! This version reads the RESOLVED graph from `cargo metadata`. It walks the
//! normal (non-dev, non-build) dependency closure of `cadus-web` across every
//! target platform, so a client that enters through any crate, any manifest
//! table, or any `cfg` fails the test.
//!
//! Three guards work together:
//!
//! 1. `web_normal_closure_carries_no_http_client_or_model_sdk` rejects the named
//!    clients and SDKs anywhere in the closure.
//! 2. The two direct-list tests pin the literal set of DIRECT normal
//!    dependencies of `cadus-web` and of `cadus-store`. A crate compiles against
//!    what its own manifest declares, so a new name in either list is a
//!    reviewable diff. `cadus-store` holds no purity test of its own, and it
//!    links into every handler, so its list is pinned here.
//! 3. `web_and_worker_sources_hold_no_socket_or_process_call` reads the SOURCE
//!    of both tiers. Guards 1 and 2 see dependencies only, and `std::net` and
//!    `std::process` are dependencies of nothing, so a handler that opens a
//!    socket by hand passed both.
//!
//! NOTE: `hyper-util` is not in `FORBIDDEN`. `axum` pulls `hyper` and
//! `hyper-util` for the SERVER side, so both sit in the closure of every axum
//! crate and a closure test on them fails on the first run. Guard 2 is what
//! stops a hand-written `hyper_util::client` call: the crate that writes it must
//! first put `hyper-util` in its own manifest.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId};

/// Crates that must never enter the normal dependency closure of `cadus-web`.
/// Each one is an outbound HTTP client, a websocket client, or a model SDK.
const FORBIDDEN: [&str; 9] = [
    "anthropic",
    "async-openai",
    "curl",
    "isahc",
    "openai",
    "openrouter",
    "reqwest",
    "tokio-tungstenite",
    "ureq",
];

/// Load the workspace metadata with the resolved dependency graph.
fn metadata() -> Metadata {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    MetadataCommand::new()
        .manifest_path(&manifest)
        .exec()
        .unwrap_or_else(|e| panic!("cargo metadata for {}: {e}", manifest.display()))
}

/// Find the package with this name.
fn package<'a>(metadata: &'a Metadata, name: &str) -> &'a Package {
    metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == name)
        .unwrap_or_else(|| panic!("cargo metadata reports no package named `{name}`"))
}

/// Collect the names in the normal dependency closure of `root`.
///
/// The walk keeps an edge whose kind list holds `Normal` and drops a dev-only or
/// build-only edge. It reads no `target` field, so a target-specific dependency
/// of any platform stays in the closure.
fn normal_closure(metadata: &Metadata, root: &str) -> BTreeSet<String> {
    let resolve = metadata
        .resolve
        .as_ref()
        .unwrap_or_else(|| panic!("cargo metadata returned no resolved dependency graph"));

    let mut edges: BTreeMap<PackageId, Vec<PackageId>> = BTreeMap::new();
    for node in &resolve.nodes {
        let mut normal = Vec::new();
        for dep in &node.deps {
            // An empty kind list is the pre-1.41 cargo format, which does not
            // separate a normal edge from a dev edge. Fail loudly: a silent
            // fallback either drops real edges or admits every dev dependency.
            assert!(
                !dep.dep_kinds.is_empty(),
                "cargo metadata reports no dependency kind for `{}` -> `{}`; this cargo is too old for the R4 guard",
                node.id,
                dep.pkg
            );
            if dep
                .dep_kinds
                .iter()
                .any(|kind| kind.kind == DependencyKind::Normal)
            {
                normal.push(dep.pkg.clone());
            }
        }
        edges.insert(node.id.clone(), normal);
    }

    let mut name_of: BTreeMap<PackageId, String> = BTreeMap::new();
    for package in &metadata.packages {
        name_of.insert(package.id.clone(), package.name.to_string());
    }

    let mut seen: BTreeSet<PackageId> = BTreeSet::new();
    let mut stack = vec![package(metadata, root).id.clone()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let next = edges
            .get(&id)
            .unwrap_or_else(|| panic!("cargo metadata resolved no node for `{id}`"));
        stack.extend(next.iter().cloned());
    }

    seen.iter()
        .map(|id| {
            name_of
                .get(id)
                .cloned()
                .unwrap_or_else(|| panic!("cargo metadata reports no package for `{id}`"))
        })
        .collect()
}

/// Collect the sorted names of the DIRECT normal dependencies of `name`.
///
/// `cargo metadata` lists a target-specific dependency beside a plain one, so
/// this reads `[target.'cfg(...)'.dependencies]` too.
fn direct_normal_dependencies(metadata: &Metadata, name: &str) -> Vec<String> {
    let mut found: Vec<String> = package(metadata, name)
        .dependencies
        .iter()
        .filter(|dep| dep.kind == DependencyKind::Normal)
        .map(|dep| dep.name.to_string())
        .collect();
    found.sort();
    found.dedup();
    found
}

#[test]
fn web_normal_closure_carries_no_http_client_or_model_sdk() {
    let metadata = metadata();
    let closure = normal_closure(&metadata, "cadus-web");

    // Prove the walk reaches past the root and past the first level. Without
    // this, a broken walk makes the loop below pass on an empty set.
    for reached in [
        "cadus-web",
        "cadus-core",
        "cadus-store",
        "axum",
        "sqlx",
        "hyper",
    ] {
        assert!(
            closure.contains(reached),
            "the dependency walk lost `{reached}`; it found {} crates",
            closure.len()
        );
    }

    for name in FORBIDDEN {
        assert!(
            !closure.contains(name),
            "R4: `{name}` is in the normal dependency closure of cadus-web, so it links into the request path"
        );
    }
}

#[test]
fn web_direct_normal_dependencies_are_the_declared_list() {
    let metadata = metadata();
    let found = direct_normal_dependencies(&metadata, "cadus-web");
    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(
        found,
        vec![
            // M5 U2 added `argon2`, `base64ct`, `getrandom`, `sha2`, `subtle`,
            // and `unicode-normalization` for the auth primitives. Every one of
            // the six is local CPU work: a hash, an encoder, a comparison, a
            // normalizer, or one `getrandom` syscall. None of them opens a
            // socket, and none of them talks to a model.
            "argon2",
            "axum",
            "base64ct",
            "cadus-core",
            "cadus-store",
            "getrandom",
            "serde",
            "serde_json",
            "sha2",
            "sqlx",
            "subtle",
            "tokio",
            // M5 U9 added `tokio-stream`. It is a stream ADAPTER: it turns the
            // broadcast receiver of the diagnosis hub into the `Stream` that
            // `axum::response::sse::Sse` takes. It opens nothing and it talks to
            // no model.
            "tokio-stream",
            "tracing",
            "tracing-subscriber",
            "unicode-normalization",
        ],
        "R4: a new normal dependency of cadus-web needs a review; update this list with it"
    );
}

#[test]
fn store_direct_normal_dependencies_are_the_declared_list() {
    let metadata = metadata();
    let found = direct_normal_dependencies(&metadata, "cadus-store");
    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(
        found,
        vec![
            "cadus-core",
            // M5 U9 added `serde`. `diagnosis::JobPayload` is the document the
            // grade transaction writes and the worker claim reads, so it derives
            // its reader and its writer. It is a data-format crate.
            "serde",
            "serde_json",
            "sqlx",
            "thiserror",
            "tokio",
            "tracing",
            "uuid"
        ],
        "R4: cadus-store links into every cadus-web handler; a new normal dependency of it needs the same review"
    );
}

/// Tokens that must not appear in the source of `cadus-web` and `cadus-worker`.
///
/// A request handler does local CPU work and DB I/O only (R4). Guard 1 and
/// guard 2 above read the dependency graph, so a handler that opens a socket by
/// hand, or that starts a child process, passes both. Each token below is one
/// such way out of the process:
///
/// - `reqwest` is an outbound HTTP client. The name also catches a `use` line
///   that a manifest edit smuggled in.
/// - `TcpStream::connect` and `UdpSocket` open a raw socket.
/// - `hyper::Client` and `hyper_util::client` build an HTTP client from the
///   server-side crates that `axum` already pulls in.
/// - `std::process::Command` and `Command::new` start a child process, which
///   runs any of the above outside the process.
const FORBIDDEN_SOURCE_TOKENS: [&str; 7] = [
    "reqwest",
    "TcpStream::connect",
    "UdpSocket",
    "hyper::Client",
    "hyper_util::client",
    "std::process::Command",
    "Command::new",
];

/// The source directories that the scan walks, relative to the repository root.
///
/// The scan reads both tiers from both purity tests, so a token in either crate
/// fails both tests.
const SCANNED_SOURCE_DIRS: [&str; 2] = ["crates/web/src", "crates/worker/src"];

/// The root of the repository.
///
/// `CARGO_MANIFEST_DIR` is `<root>/crates/<crate>`, so the root is two levels up.
fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("`{}` has no grandparent", manifest_dir.display()))
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
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", current.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("read_dir {}: {e}", current.display()));
            let path = entry.path();
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

/// R4 at the source level: no handler opens a socket or starts a process.
///
/// Round-4 finding on the accepted list: the R4 purity guards pin the resolved
/// dependency graph, not the handler bodies. `std::net` and `std::process` need
/// no dependency at all, so a hand-written connect passed every guard above.
#[test]
fn web_and_worker_sources_hold_no_socket_or_process_call() {
    let root = repo_root();
    let mut scanned: Vec<PathBuf> = Vec::new();
    let mut hits: Vec<String> = Vec::new();

    for dir in SCANNED_SOURCE_DIRS {
        let full = root.join(dir);
        let files = rust_sources(&full);
        assert!(
            !files.is_empty(),
            "the source scan found no `.rs` file under `{dir}`; the walk is broken"
        );
        for file in files {
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
            let shown = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .display()
                .to_string();
            for (index, line) in text.lines().enumerate() {
                for token in FORBIDDEN_SOURCE_TOKENS {
                    if line.contains(token) {
                        hits.push(format!("{shown}:{}: {token}: {}", index + 1, line.trim()));
                    }
                }
            }
            scanned.push(file);
        }
    }

    // Prove the walk read the four known source files. Without this, a walk that
    // returns nothing makes the check above pass on an empty set.
    for expected in [
        "crates/web/src/lib.rs",
        "crates/web/src/bin/cadus-web.rs",
        "crates/worker/src/lib.rs",
        "crates/worker/src/bin/cadus-worker.rs",
    ] {
        let wanted = root.join(expected);
        assert!(
            scanned.contains(&wanted),
            "the source scan missed `{expected}`; it read {} files",
            scanned.len()
        );
    }

    assert!(
        hits.is_empty(),
        "R4: a handler does local CPU work and DB I/O only; these lines leave the process:\n{}",
        hits.join("\n")
    );
}
