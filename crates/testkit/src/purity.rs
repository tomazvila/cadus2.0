//! The R4 dependency guards: an HTTP client or a model SDK on a tier is a
//! reviewable diff.
//!
//! The guards read the RESOLVED graph from `cargo metadata`, so a dependency
//! under `[target.'cfg(...)'.dependencies]` of any platform stays in the
//! closure. A walk keeps an edge whose kind list holds `Normal` and drops a
//! dev-only or build-only edge, so this crate is outside every guard.

use std::collections::{BTreeMap, BTreeSet};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, NodeDep, Package, PackageId};

use crate::stop_on;

/// Crates that must never enter the normal dependency closure of a tier.
/// Each one is an outbound HTTP client, a websocket client, or a model SDK.
///
/// M5 U10 gave the worker its model call, and the list did NOT shrink for it.
/// The call lives in `cadus-model-client`, which speaks HTTP/1.1 on
/// `hyper::client::conn`, a name this list never carried, because `axum` pulls
/// `hyper` for the server half of `cadus-web` anyway. Every crate below is
/// still a way to reach the network that no review has approved.
pub const FORBIDDEN_CLIENTS: [&str; 9] = [
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

/// The sorted DIRECT normal dependencies of `cadus-store`.
///
/// `cadus-store` links into every `cadus-web` handler and into `cadus-worker`,
/// so every tier pins this list beside its own.
pub const STORE_DIRECT_DEPENDENCIES: [&str; 8] = [
    "cadus-core",
    // M5 U9 added `serde`. `diagnosis::JobPayload` is the document the grade
    // transaction writes and the worker claim reads, so it derives its reader
    // and its writer. It is a data-format crate: it opens nothing and it talks
    // to no model.
    "serde",
    "serde_json",
    "sqlx",
    "thiserror",
    "tokio",
    "tracing",
    "uuid",
];

/// Load the workspace metadata with the resolved dependency graph.
pub fn metadata() -> Metadata {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    stop_on(
        format!("cargo metadata for {}", manifest.display()),
        MetadataCommand::new().manifest_path(&manifest).exec(),
    )
}

/// Find the package with this name.
pub fn package<'a>(metadata: &'a Metadata, name: &str) -> &'a Package {
    stop_on(
        format!("cargo metadata reports no package named `{name}`"),
        metadata
            .packages
            .iter()
            .find(|package| package.name.as_ref() == name)
            .ok_or("no such package"),
    )
}

/// Whether the edge `from` -> `dep` holds a `Normal` kind.
///
/// An empty kind list is the pre-1.41 cargo format, which does not separate a
/// normal edge from a dev edge. The function stops the test on one: a silent
/// fallback either drops real edges or admits every dev dependency.
pub fn is_normal_edge(from: &PackageId, dep: &NodeDep) -> bool {
    assert!(
        !dep.dep_kinds.is_empty(),
        "cargo metadata reports no dependency kind for `{from}` -> `{}`; this cargo is too old for the R4 guard",
        dep.pkg
    );
    dep.dep_kinds
        .iter()
        .any(|kind| kind.kind == DependencyKind::Normal)
}

/// Collect the names in the normal dependency closure of `root`.
///
/// The walk reads no `target` field, so a target-specific dependency of any
/// platform stays in the closure.
pub fn normal_closure(metadata: &Metadata, root: &str) -> BTreeSet<String> {
    let resolve = metadata
        .resolve
        .as_ref()
        .expect("cargo metadata returned no resolved dependency graph");

    let mut edges: BTreeMap<PackageId, Vec<PackageId>> = BTreeMap::new();
    for node in &resolve.nodes {
        let normal: Vec<PackageId> = node
            .deps
            .iter()
            .filter(|dep| is_normal_edge(&node.id, dep))
            .map(|dep| dep.pkg.clone())
            .collect();
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
        let next = stop_on(
            format!("cargo metadata resolved no node for `{id}`"),
            edges.get(&id).ok_or("no such node"),
        );
        stack.extend(next.iter().cloned());
    }

    seen.iter()
        .map(|id| {
            stop_on(
                format!("cargo metadata reports no package for `{id}`"),
                name_of.get(id).cloned().ok_or("no such package"),
            )
        })
        .collect()
}

/// Collect the sorted names of the DIRECT normal dependencies of `name`.
///
/// `cargo metadata` lists a target-specific dependency beside a plain one, so
/// this reads `[target.'cfg(...)'.dependencies]` too.
pub fn direct_normal_dependencies(metadata: &Metadata, name: &str) -> Vec<String> {
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

/// Check the normal dependency closure of `root`: it holds every name of
/// `reached` and no name of `forbidden`.
///
/// `reached` proves the walk goes past the root and past the first level.
/// Without it, a broken walk makes the `forbidden` loop pass on an empty set.
/// `reason` completes the failure line of a forbidden name after the name.
pub fn check_closure(root: &str, reached: &[&str], forbidden: &[&str], reason: &str) {
    let metadata = metadata();
    let closure = normal_closure(&metadata, root);
    for name in reached {
        assert!(
            closure.contains(*name),
            "the dependency walk lost `{name}`; it found {} crates",
            closure.len()
        );
    }
    for name in forbidden {
        assert!(!closure.contains(*name), "R4: `{name}` {reason}");
    }
}

/// Check that the sorted DIRECT normal dependencies of `name` are exactly
/// `expected`. `reason` is the failure line.
pub fn check_direct_list(name: &str, expected: &[&str], reason: &str) {
    let metadata = metadata();
    let found = direct_normal_dependencies(&metadata, name);
    let found: Vec<&str> = found.iter().map(String::as_str).collect();
    assert_eq!(found, expected, "{reason}");
}

#[cfg(test)]
mod tests {
    use cargo_metadata::{NodeDep, PackageId};
    use serde_json::json;

    use super::{
        FORBIDDEN_CLIENTS, STORE_DIRECT_DEPENDENCIES, check_closure, check_direct_list,
        is_normal_edge, metadata, package,
    };

    /// One resolved edge from the JSON of `cargo metadata`, with these kinds.
    fn edge(kinds: &[&str]) -> NodeDep {
        let kinds: Vec<_> = kinds
            .iter()
            .map(|kind| json!({"kind": kind, "target": null}))
            .collect();
        serde_json::from_value(json!({
            "name": "dep",
            "pkg": "registry+https://example.test#dep@1.0.0",
            "dep_kinds": kinds,
        }))
        .unwrap()
    }

    #[test]
    fn a_normal_edge_stays_and_a_dev_edge_goes() {
        let from = PackageId {
            repr: "path+file:///x#root@0.1.0".to_string(),
        };
        assert!(is_normal_edge(&from, &edge(&["dev", "normal"])));
        assert!(!is_normal_edge(&from, &edge(&["dev"])));
    }

    #[test]
    #[should_panic(expected = "reports no dependency kind")]
    fn an_edge_without_a_kind_stops_the_guard() {
        let from = PackageId {
            repr: "path+file:///x#root@0.1.0".to_string(),
        };
        is_normal_edge(&from, &edge(&[]));
    }

    #[test]
    #[should_panic(expected = "no package named `no-such-crate`")]
    fn an_unknown_package_stops_the_guard() {
        package(&metadata(), "no-such-crate");
    }

    /// The guards pass on this crate itself: its closure reaches its own
    /// dependencies and no client, and its direct list is the manifest.
    #[test]
    fn the_testkit_passes_its_own_guards() {
        check_closure(
            "cadus-testkit",
            &["cadus-testkit", "cargo_metadata", "serde_json", "tokio"],
            &FORBIDDEN_CLIENTS,
            "is in the normal dependency closure of cadus-testkit",
        );
        check_direct_list(
            "cadus-testkit",
            &["cargo_metadata", "serde", "serde_json", "tokio"],
            "the manifest of cadus-testkit changed",
        );
        assert_eq!(STORE_DIRECT_DEPENDENCIES.len(), 8);
    }

    #[test]
    #[should_panic(expected = "the dependency walk lost `no-such-crate`")]
    fn a_lost_name_stops_the_closure_guard() {
        check_closure("cadus-testkit", &["no-such-crate"], &[], "");
    }

    #[test]
    #[should_panic(expected = "R4: `serde_json` is in the closure")]
    fn a_forbidden_name_in_the_closure_stops_the_guard() {
        check_closure("cadus-testkit", &[], &["serde_json"], "is in the closure");
    }

    #[test]
    #[should_panic(expected = "the list moved")]
    fn a_direct_list_of_another_shape_stops_the_guard() {
        check_direct_list("cadus-testkit", &["serde"], "the list moved");
    }
}
