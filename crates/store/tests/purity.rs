//! R4 — an HTTP client or a model SDK in the store is a reviewable diff.
//!
//! `cadus-store` is the Postgres adapter. It links into every `cadus-web`
//! handler and into `cadus-worker`, so a client that enters here reaches both
//! binaries.
//!
//! `crates/web/tests/purity.rs` pins the direct dependency list of this crate
//! too, because the web request path carries it. This file gives the store its
//! own pin, so the guard survives a change of the web crate and names the
//! store in its own failure message.
//!
//! Two guards work together:
//!
//! 1. `store_normal_closure_carries_no_http_client_or_model_sdk` rejects the
//!    named clients and SDKs anywhere in the normal dependency closure.
//! 2. `store_direct_normal_dependencies_are_the_literal_list` pins the literal
//!    sorted set of DIRECT normal dependencies. A crate compiles against what
//!    its own manifest declares, so a new name in that list is a reviewable
//!    diff.
//!
//! Both guards read the resolved graph through `cadus_testkit::purity`, which
//! keeps a `Normal` edge and drops a dev-only or build-only edge, so the
//! `cadus-testkit` dev-dependency of this crate is outside both.

use cadus_testkit::purity::{
    FORBIDDEN_CLIENTS, STORE_DIRECT_DEPENDENCIES, check_closure, check_direct_list,
};

#[test]
fn store_normal_closure_carries_no_http_client_or_model_sdk() {
    check_closure(
        "cadus-store",
        &[
            "cadus-store",
            "cadus-core",
            "sqlx",
            "sqlx-postgres",
            "tokio",
        ],
        &FORBIDDEN_CLIENTS,
        "is in the normal dependency closure of cadus-store, so it links into every handler and into the worker",
    );
}

#[test]
fn store_direct_normal_dependencies_are_the_literal_list() {
    check_direct_list(
        "cadus-store",
        &STORE_DIRECT_DEPENDENCIES,
        "R4: a new normal dependency of cadus-store needs a review; update this list with it",
    );
}
