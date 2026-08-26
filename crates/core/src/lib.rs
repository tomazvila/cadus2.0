//! Pure scheduling and pedagogy core: no network, no database, no model calls (R3).

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod answer;
pub mod config;
pub mod curriculum;
pub mod event;
pub mod fire;
pub mod learner;
pub mod numeric;
pub mod xp;
