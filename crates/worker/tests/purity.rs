//! R4 — a model/HTTP client dependency on a request-handling crate is a
//! reviewable diff.
//!
//! The worker is the tier that calls a model, and M0 gives it no client yet.
//! This test pins the dependency set, so the client of a later milestone arrives
//! as a visible diff line and not as a silent transitive addition. The gate
//! proved R3 for the core crate and proved R4 nowhere (finding #22). This test
//! reads `crates/worker/Cargo.toml` and compares the `[dependencies]` key set
//! with the literal list below.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

/// Names that must never appear in `[dependencies]` of `cadus-worker`.
const FORBIDDEN: [&str; 8] = [
    "reqwest",
    "hyper-util",
    "ureq",
    "async-openai",
    "anthropic",
    "openai",
    "curl",
    "isahc",
];

/// Read and parse `crates/worker/Cargo.toml`.
fn manifest() -> toml::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Collect the keys of `[dependencies]` in sorted order.
fn runtime_dependency_keys(manifest: &toml::Value) -> Vec<String> {
    let table = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("crates/worker/Cargo.toml has no [dependencies] table"));
    let mut keys: Vec<String> = table.keys().cloned().collect();
    keys.sort();
    keys
}

#[test]
fn worker_dependency_set_is_exactly_the_declared_list() {
    let found = runtime_dependency_keys(&manifest());
    assert_eq!(
        found,
        vec![
            "cadus-core".to_string(),
            "cadus-store".to_string(),
            "sqlx".to_string(),
            "thiserror".to_string(),
            "tokio".to_string(),
            "tracing".to_string(),
            "tracing-subscriber".to_string(),
        ],
        "R4: a new [dependencies] entry of cadus-worker needs a review; update this list with it"
    );
}

#[test]
fn worker_declares_no_http_client_and_no_model_sdk() {
    let found = runtime_dependency_keys(&manifest());
    for name in FORBIDDEN {
        assert!(
            !found.iter().any(|key| key == name),
            "R4: cadus-worker must not depend on `{name}`; [dependencies] holds {found:?}"
        );
    }
}
