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
//! Two guards work together:
//!
//! 1. `web_normal_closure_carries_no_http_client_or_model_sdk` rejects the named
//!    clients and SDKs anywhere in the closure.
//! 2. The two direct-list tests pin the literal set of DIRECT normal
//!    dependencies of `cadus-web` and of `cadus-store`. A crate compiles against
//!    what its own manifest declares, so a new name in either list is a
//!    reviewable diff. `cadus-store` holds no purity test of its own, and it
//!    links into every handler, so its list is pinned here.
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
            "axum",
            "cadus-core",
            "cadus-store",
            "serde_json",
            "sqlx",
            "tokio",
            "tracing",
            "tracing-subscriber",
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
            "sqlx",
            "thiserror",
            "tokio",
            "tracing",
            "uuid"
        ],
        "R4: cadus-store links into every cadus-web handler; a new normal dependency of it needs the same review"
    );
}
