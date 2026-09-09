//! R4 — an HTTP client or a model SDK is a reviewable diff, on the worker too.
//!
//! The worker is the tier that calls a model, and M0 gives it no client yet.
//! This test pins the dependency graph, so the client of a later milestone
//! arrives as a visible diff line and not as a silent transitive addition.
//!
//! Round-1 finding #22 asked for this guard. The first version read the literal
//! `[dependencies]` table of one manifest, and round-4 finding #6 showed two
//! ways past it: a client under `[target.'cfg(unix)'.dependencies]` of this
//! crate, and a client in `cadus-store`, which the worker imports. Both link
//! into the binary.
//!
//! This version reads the RESOLVED graph from `cargo metadata` through
//! `cadus_testkit::purity`. It walks the normal (non-dev, non-build)
//! dependency closure of `cadus-worker` across every target platform, so a
//! client that enters through any crate, any manifest table, or any `cfg`
//! fails the test.
//!
//! Three guards work together:
//!
//! 1. `worker_normal_closure_carries_no_http_client_or_model_sdk` rejects the
//!    named clients and SDKs anywhere in the closure.
//! 2. The two direct-list tests pin the literal set of DIRECT normal
//!    dependencies of `cadus-worker` and of `cadus-store`. A crate compiles
//!    against what its own manifest declares, so a new name in either list is a
//!    reviewable diff. Both tiers link `cadus-store`, so
//!    `crates/web/tests/purity.rs` pins the same list.
//! 3. `web_and_worker_sources_hold_no_socket_or_process_call` reads the SOURCE
//!    of both tiers. Guards 1 and 2 see dependencies only, and `std::net` and
//!    `std::process` are dependencies of nothing, so a body that opens a socket
//!    by hand passed both. `crates/web/tests/purity.rs` runs the same scan.
//!
//! NOTE: `hyper-util` is not in `FORBIDDEN_CLIENTS`. `axum` pulls `hyper` and
//! `hyper-util` for the SERVER side of `cadus-web`, so the two lists stay the
//! same on both tiers. Guard 2 stops a hand-written `hyper_util::client` call:
//! the crate that writes it must first put `hyper-util` in its own manifest.

use cadus_testkit::purity::{
    FORBIDDEN_CLIENTS, STORE_DIRECT_DEPENDENCIES, check_closure, check_direct_list,
};
use cadus_testkit::sources::{
    FORBIDDEN_SOURCE_TOKENS, TIER_ROOT_FILES, TIER_SOURCE_DIRS, check_sources,
};

#[test]
fn worker_normal_closure_carries_no_http_client_or_model_sdk() {
    check_closure(
        "cadus-worker",
        &[
            "cadus-worker",
            "cadus-core",
            "cadus-store",
            "sqlx",
            "sqlx-postgres",
        ],
        &FORBIDDEN_CLIENTS,
        "is in the normal dependency closure of cadus-worker; the ONE approved client is cadus-model-client",
    );
}

#[test]
fn worker_direct_normal_dependencies_are_the_declared_list() {
    check_direct_list(
        "cadus-worker",
        &[
            "cadus-core",
            // M5 U10 added `cadus-model-client`. It is the ONE crate of the
            // workspace that opens an outbound socket, and this is the ONE
            // manifest that names it: L6 makes a model call from `cadus-web` a
            // compile error, not a review note.
            "cadus-model-client",
            "cadus-store",
            // Typed recipe catalogs deserialize local data without network access.
            "serde",
            // M5 U10 added `serde_json`. The diagnosis payload, the tool schema
            // and the result document are JSON documents. It is a data-format
            // crate: no socket, no model.
            "serde_json",
            // M6 R1 added `sha2`. The authoring prompt digest is a SHA-256, the
            // way 1.0's `template_prompt_digest` is. It is a pure computation
            // crate: no socket, no model.
            "sha2",
            "sqlx",
            "thiserror",
            "tokio",
            "tracing",
            "tracing-subscriber",
        ],
        "R4: a new normal dependency of cadus-worker needs a review; update this list with it",
    );
}

#[test]
fn store_direct_normal_dependencies_are_the_declared_list() {
    check_direct_list(
        "cadus-store",
        &STORE_DIRECT_DEPENDENCIES,
        "R4: cadus-store links into cadus-worker; a new normal dependency of it needs the same review",
    );
}

/// R4 at the source level: no handler opens a socket or starts a process.
///
/// Round-4 finding on the accepted list: the R4 purity guards pin the resolved
/// dependency graph, not the handler bodies. `std::net` and `std::process` need
/// no dependency at all, so a hand-written connect passed every guard above.
#[test]
fn web_and_worker_sources_hold_no_socket_or_process_call() {
    check_sources(
        &TIER_SOURCE_DIRS,
        &FORBIDDEN_SOURCE_TOKENS,
        &TIER_ROOT_FILES,
    );
}
