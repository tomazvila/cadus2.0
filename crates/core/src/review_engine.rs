//! Compiled identity of the review/render/check engine.

/// SHA-256 over the versioned, sorted build input manifest emitted by build.rs.
pub const DIGEST: &str = env!("CADUS_REVIEW_ENGINE_DIGEST");
