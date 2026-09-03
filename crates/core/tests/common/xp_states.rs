//! The topic states of the 1.0 XP progress tests over the mini curriculum.

use std::collections::BTreeMap;

use cadus_core::event::TopicStatus;
use cadus_core::learner::TopicState;

use super::{MINI_FRACTIONS, MINI_NUMBERS};

/// The 12 topic ids of the mini curriculum, sorted, as the 1.0 test sorts them.
#[must_use]
pub fn mini_ids_sorted() -> Vec<String> {
    let mut ids: Vec<String> = MINI_NUMBERS
        .iter()
        .chain(MINI_FRACTIONS.iter())
        .map(|id| (*id).to_owned())
        .collect();
    ids.sort();
    ids
}

/// The 1.0 states of the progress tests: the first `mastered` sorted topics are
/// `learning`, the rest are `untouched`.
#[must_use]
pub fn mini_states(mastered: usize) -> BTreeMap<String, TopicState> {
    mini_ids_sorted()
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let status = if index < mastered {
                TopicStatus::Learning
            } else {
                TopicStatus::Untouched
            };
            (
                id,
                TopicState {
                    status,
                    ..TopicState::default()
                },
            )
        })
        .collect()
}
