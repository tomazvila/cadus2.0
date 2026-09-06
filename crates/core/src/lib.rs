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
pub mod diagnostic;
pub mod event;
pub mod fire;
pub mod instruction;
pub mod integrated;
pub mod learner;
pub mod numeric;
pub mod pool;
pub mod projector;
pub mod readiness;
pub mod selector;
pub mod template;
pub mod xp;
