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
//! This version reads the RESOLVED graph from `cargo metadata` through
//! `cadus_testkit::purity`. It walks the normal (non-dev, non-build) dependency
//! closure of `cadus-web` across every target platform, so a client that enters
//! through any crate, any manifest table, or any `cfg` fails the test.
//!
//! Four guards work together:
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
//!    socket by hand passed both. `crates/worker/tests/purity.rs` runs the same
//!    scan.
//! 4. M5 U12 adds the L6 boundary and its positive control at the end of this
//!    file: `cadus-web` and `cadus-store` must NOT hold `cadus-model-client`,
//!    `cadus-worker` MUST, and the three tests read one walk. Guard 1 covers the
//!    web closure alone, so a client that entered through `cadus-store` was
//!    caught there and never named; guard 4 names the store, the manifests, and
//!    the crate that proves the walk still works.
//!
//! NOTE: `hyper-util` is not in the forbidden list. `axum` pulls `hyper` and
//! `hyper-util` for the SERVER side, so both sit in the closure of every axum
//! crate and a closure test on them fails on the first run. Guard 2 is what
//! stops a hand-written `hyper_util::client` call: the crate that writes it must
//! first put `hyper-util` in its own manifest.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_testkit::purity::{
    FORBIDDEN_CLIENTS, STORE_DIRECT_DEPENDENCIES, check_closure, check_direct_list, metadata,
    normal_closure, package,
};
use cadus_testkit::sources::{
    FORBIDDEN_SOURCE_TOKENS, TIER_ROOT_FILES, TIER_SOURCE_DIRS, check_sources,
};
use cargo_metadata::Metadata;

/// The ONE crate of the workspace that reaches a model endpoint (L6, T2).
const MODEL_CLIENT: &str = "cadus-model-client";

/// The crates that must never enter the normal dependency closure of
/// `cadus-web`: every outbound client of the workspace list, and the model
/// client that L6 keeps off the request path.
///
/// A `cadus-model-client` dependency in `cadus-web`, direct or through any
/// crate it imports, fails guard 1 here and guard 4 below.
fn forbidden_on_the_request_path() -> Vec<&'static str> {
    let mut names = FORBIDDEN_CLIENTS.to_vec();
    names.push(MODEL_CLIENT);
    names
}

#[test]
fn web_normal_closure_carries_no_http_client_or_model_sdk() {
    check_closure(
        "cadus-web",
        &[
            "cadus-web",
            "cadus-core",
            "cadus-store",
            "axum",
            "sqlx",
            "hyper",
        ],
        &forbidden_on_the_request_path(),
        "is in the normal dependency closure of cadus-web, so it links into the request path",
    );
}

#[test]
fn web_direct_normal_dependencies_are_the_declared_list() {
    check_direct_list(
        "cadus-web",
        &[
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
        "R4: a new normal dependency of cadus-web needs a review; update this list with it",
    );
}

#[test]
fn store_direct_normal_dependencies_are_the_declared_list() {
    check_direct_list(
        "cadus-store",
        &STORE_DIRECT_DEPENDENCIES,
        "R4: cadus-store links into every cadus-web handler; a new normal dependency of it needs the same review",
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

// ---------------------------------------------------------------------------
// Guard 4 — the L6 crate boundary, with its positive control (M5 U12)
// ---------------------------------------------------------------------------

/// The crates of the REQUEST path.
///
/// `cadus-web` answers requests, and `cadus-store` links into every one of its
/// handlers, so L6 binds both: a model call a learner waits on must be a build
/// failure and not a review note (spec section 8, the L6 row).
const REQUEST_PATH_CRATES: [&str; 2] = ["cadus-web", "cadus-store"];

/// The crate that MAY link the model client. It answers no request (R4, T2).
const MODEL_CALLER: &str = "cadus-worker";

/// Whether `name` is in the normal dependency closure of `root`.
///
/// The three tests below share this one function, so the test that proves the
/// walk FINDS the model client and the tests that require it absent read the
/// same answer from the same code. A walk that reports nothing therefore fails
/// the positive control instead of passing the two boundary tests on an empty
/// set.
fn closure_holds(metadata: &Metadata, root: &str, name: &str) -> bool {
    normal_closure(metadata, root).contains(name)
}

/// L6: neither request-path crate links the model client.
///
/// This is the test spec section 11 names in the U12 row: "the L6 test fails
/// when a handler crate is given a model-client dependency". A
/// `cadus-model-client` line in `crates/web/Cargo.toml` or in
/// `crates/store/Cargo.toml` — direct, transitive, or under any `cfg` — puts the
/// name in the closure and fails here.
#[test]
fn no_request_path_crate_links_the_model_client() {
    let metadata = metadata();
    for root in REQUEST_PATH_CRATES {
        assert!(
            !closure_holds(&metadata, root, MODEL_CLIENT),
            "L6: `{MODEL_CLIENT}` is in the normal dependency closure of `{root}`, so a model \
             call can run on the request path"
        );
    }
}

/// The positive control of the test above: the walk DOES find the client.
///
/// `cadus-worker` declares `cadus-model-client` in its own manifest, so the same
/// walk over the same graph reports `true` for it. Without this test a broken
/// walk — a lost edge, an empty resolve, a renamed package — would make
/// `no_request_path_crate_links_the_model_client` pass while proving nothing.
/// The pair is the whole L6 guard: one crate must hold the client and two must
/// not.
#[test]
fn the_l6_walk_finds_the_model_client_in_the_worker() {
    let metadata = metadata();
    assert!(
        closure_holds(&metadata, MODEL_CALLER, MODEL_CLIENT),
        "the L6 walk did not find `{MODEL_CLIENT}` in the closure of `{MODEL_CALLER}`, which \
         declares it; the walk is broken, so the two boundary tests prove nothing"
    );
    // The store is the crate both tiers link, and it is the shortest path a
    // model client could take onto the request path. Naming it here keeps the
    // control and the boundary on the same two crates.
    assert!(
        closure_holds(&metadata, MODEL_CALLER, "cadus-store"),
        "the L6 walk lost `cadus-store` in the closure of `{MODEL_CALLER}`"
    );
}

/// The model client crate is the only member of the workspace that carries an
/// outbound client, and no member of the request path names it.
///
/// The two tests above read the resolved graph. This one reads the MANIFESTS of
/// the two request-path crates, so a `cadus-model-client` line that a feature
/// flag or a `cfg` keeps out of one resolve still fails a test.
#[test]
fn no_request_path_manifest_names_the_model_client() {
    let metadata = metadata();
    for root in REQUEST_PATH_CRATES {
        let declared: Vec<String> = package(&metadata, root)
            .dependencies
            .iter()
            .map(|dep| dep.name.to_string())
            .collect();
        assert!(
            !declared.iter().any(|name| name == MODEL_CLIENT),
            "L6: `{root}` declares `{MODEL_CLIENT}` in its manifest; the request path links no \
             model client, under any dependency kind and any target"
        );
    }
}
