//! R3 core purity: the core crate declares no network, database, or model dependency.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;

/// Names that must never appear in a dependency table of `cadus-core`.
const FORBIDDEN: [&str; 6] = ["tokio", "sqlx", "axum", "hyper", "reqwest", "tower"];

/// Read and parse `crates/core/Cargo.toml`.
fn manifest() -> toml::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Collect the keys of `[dependencies]` in declaration order.
fn runtime_dependency_keys(manifest: &toml::Value) -> Vec<String> {
    let table = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("crates/core/Cargo.toml has no [dependencies] table"));
    table.keys().cloned().collect()
}

/// Collect the keys of every dependency table, including dev, build, and target tables.
fn all_dependency_keys(manifest: &toml::Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut collect = |value: Option<&toml::Value>| {
        if let Some(table) = value.and_then(toml::Value::as_table) {
            for key in table.keys() {
                keys.insert(key.clone());
            }
        }
    };
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        collect(manifest.get(name));
    }
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for spec in targets.values() {
            for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
                collect(spec.get(name));
            }
        }
    }
    keys
}

#[test]
fn core_dependency_set_is_exactly_the_thirteen_pure_crates() {
    let manifest = manifest();
    let mut found = runtime_dependency_keys(&manifest);
    found.sort();
    assert_eq!(
        found,
        vec![
            "chrono".to_string(),
            "chrono-tz".to_string(),
            "indexmap".to_string(),
            "num-bigint".to_string(),
            "num-integer".to_string(),
            "num-rational".to_string(),
            "num-traits".to_string(),
            "serde".to_string(),
            "serde_json".to_string(),
            "serde_norway".to_string(),
            "sha1".to_string(),
            "sha2".to_string(),
            "thiserror".to_string()
        ],
        "R3: [dependencies] of cadus-core must be exactly chrono, chrono-tz, indexmap, num-bigint, num-integer, num-rational, num-traits, serde, serde_json, serde_norway, sha1, sha2, thiserror"
    );
}

#[test]
fn core_declares_no_network_or_database_dependency() {
    let manifest = manifest();
    let keys = all_dependency_keys(&manifest);
    for name in FORBIDDEN {
        assert!(
            !keys.contains(name),
            "R3: cadus-core must not depend on `{name}`; dependency tables hold {keys:?}"
        );
    }
}
