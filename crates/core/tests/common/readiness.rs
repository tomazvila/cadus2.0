//! Shared assertions for curriculum recipe-readiness cohorts.

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

use super::paths::curriculum_root;

/// Assert the production readiness invariants for each named knowledge point.
#[track_caller]
pub fn assert_kps_ready<I, K>(keys: I)
where
    I: IntoIterator<Item = K>,
    K: AsRef<str>,
{
    let (curriculum, findings) = load_curriculum(&curriculum_root()).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let index = ReadinessIndex::build(&curriculum);
    let mut failures = Vec::new();
    for key in keys {
        let key = key.as_ref();
        let Some(facts) = index.get(key) else {
            failures.push(format!("{key}: missing from the curriculum"));
            continue;
        };
        if facts.decidable.len() < 4
            || facts.held_out.is_none()
            || facts.practice_exemplars() < 3
            || !facts.solutions
        {
            failures.push(format!(
                "{key}: decidable={}, practice={}, held_out={}, solutions={}",
                facts.decidable.len(),
                facts.practice_exemplars(),
                facts.held_out.is_some(),
                facts.solutions
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
