//! The learner model: the parity blob and its digest (D3, spec section 3).
//!
//! Every expected value below is a literal from `docs/reference/projector-1.0-spec.md`
//! section 9.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;

use cadus_core::event::{EventError, Timestamp};
use cadus_core::learner::{LearnerModel, TopicState};

/// A timestamp outside the representable range does not serialize, so the blob
/// and the digest both report the error instead of a wrong digest.
#[test]
fn a_timestamp_outside_the_range_fails_the_digest() {
    let mut topics = BTreeMap::new();
    topics.insert(
        "addition".to_owned(),
        TopicState {
            t0: Some(Timestamp::from_micros(i64::MAX)),
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    assert!(matches!(model.parity_blob(), Err(EventError::Serialize(_))));
    assert!(matches!(
        model.parity_digest(),
        Err(EventError::Serialize(_))
    ));

    let empty = LearnerModel::default();
    assert_eq!(
        empty.parity_digest().unwrap().len(),
        64,
        "a default model digests to 64 hex characters"
    );
}
