//! The content fixtures of the admin write tests: the digest under review,
//! the prompt digest of the template fixture, and the template document.

use cadus_store::content::{KIND_TEMPLATE, NewDocument};

use super::KP;

/// The digest of the document under review.
pub const DIGEST: &str = "sha256:0123456789abcdef";

/// The prompt digest the template fixture carries (spec section 2.2, "Prompt
/// digest"). The value is a literal of the tests and never a value the code
/// under test computes.
pub const PROMPT_DIGEST: &str = "sha256:aaaabbbbccccdddd";

/// One template document to insert.
pub fn template<'a>(digest: &'a str, body: &'a serde_json::Value) -> NewDocument<'a> {
    NewDocument {
        digest,
        kp_id: KP,
        kind: KIND_TEMPLATE,
        body,
        authoring_attempts: 2,
        cost_usd: Some("0.004500"),
        prompt_digest: Some(PROMPT_DIGEST),
    }
}
