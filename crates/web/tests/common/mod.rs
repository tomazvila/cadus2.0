//! Fixtures the `/api/auth/*` test files and the guarded route test files share.
//!
//! The module holds no assertion. Every expected value is a literal of the test
//! file that reads it (HANDOVER section 3).
//!
//! **The token digests below are literals, not computed values.** A fixture that
//! called `hash_token` to seed a row would agree with a broken `hash_token`, so
//! each pair here is a raw token and the SHA-256 hex that `sha256sum` gives for
//! it. Seeding with the literal digest and presenting the raw token proves the
//! production digest matches.

#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod arena;
mod bench;
mod callback;
mod corpus;
mod db;
mod events;
mod fault;
mod http;
mod live;
mod oauth;
mod oauth_url;
mod plan;
mod process;
mod quiz;
mod recovery;
mod route;
mod seed;
mod task;

pub use arena::*;
pub use bench::*;
pub use callback::*;
pub use corpus::*;
pub use db::*;
pub use events::*;
pub use fault::*;
pub use http::*;
pub use live::*;
pub use oauth::*;
pub use oauth_url::*;
pub use plan::*;
pub use process::*;
pub use quiz::*;
pub use recovery::*;
pub use route::*;
pub use seed::*;
pub use task::*;
