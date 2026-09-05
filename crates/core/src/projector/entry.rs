//! The entry points of the fold, the parity blob, and the lesson
//! knowledge-point gates (`projector.py:773-872`).

use crate::config::Config;
use crate::curriculum::{Curriculum, render_json, sha256_hex};
use crate::event::{Event, Timestamp};
use crate::learner::LearnerModel;

use super::state::serialize_error;
use super::{DEFAULT_XP_GOAL, Projector, ProjectorError, apply_regrades};

/// The inputs both entry points share.
///
/// The 1.0 signature is `project(events, graph, cfg, now=, tz=, goal=)`
/// (`projector.py:773-791`); this struct carries the same five.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionInput<'a> {
    /// The curriculum the fold reads.
    pub graph: &'a Curriculum,
    /// The scheduler config the fold reads.
    pub cfg: &'a Config,
    /// The wall-clock instant of the BUILD. Only `built_from_ts` carries it.
    pub now: Timestamp,
    /// The time zone of the day boundary. `None` means UTC.
    pub tz: Option<&'a str>,
    /// The daily XP goal the streak compares against.
    pub goal: i64,
}

impl<'a> ProjectionInput<'a> {
    /// The 1.0 defaults: UTC and a goal of 40.
    #[must_use]
    pub const fn new(graph: &'a Curriculum, cfg: &'a Config, now: Timestamp) -> Self {
        Self {
            graph,
            cfg,
            now,
            tz: None,
            goal: DEFAULT_XP_GOAL,
        }
    }

    /// Set the time zone name.
    #[must_use]
    pub const fn with_timezone(mut self, tz: Option<&'a str>) -> Self {
        self.tz = tz;
        self
    }

    /// Set the daily XP goal.
    #[must_use]
    pub const fn with_goal(mut self, goal: i64) -> Self {
        self.goal = goal;
        self
    }
}

/// Full replay (`projector.py:773-791`): fold every event with FIRe applied.
///
/// # Errors
///
/// Returns [`ProjectorError`] for an unknown time zone, an unrepresentable instant, or
/// a config that does not serialize.
pub fn project(
    events: &[Event],
    input: &ProjectionInput<'_>,
) -> Result<LearnerModel, ProjectorError> {
    let mut proj = Projector::new(input.graph, input.cfg)
        .with_timezone(input.tz)?
        .with_goal(input.goal);
    for event in apply_regrades(events) {
        proj.apply(&event, true);
    }
    proj.finalize(input.now)
}

/// Incremental projection (`projector.py:794-832`).
///
/// The FIRe topic states are seeded from `cached`, the prior events replay for their
/// light indices only, and the new events apply with FIRe. The result equals [`project`]
/// over the whole stream without running the FIRe math on every earlier event.
///
/// Corrections fold over the WHOLE stream, never over each half: a `regraded` event in
/// `new` supersedes grades in `prior`, and the prior half is not inert, because its
/// result XP is tallied with FIRe off. The halves are then split again on the corrected
/// stream, which is shorter by exactly the corrections it consumed.
///
/// # Errors
///
/// Returns [`ProjectorError`] for an unknown time zone, an unrepresentable instant, or
/// a config that does not serialize.
pub fn project_incremental(
    cached: &LearnerModel,
    prior: &[Event],
    new: &[Event],
    input: &ProjectionInput<'_>,
) -> Result<LearnerModel, ProjectorError> {
    let mut whole: Vec<Event> = Vec::with_capacity(prior.len() + new.len());
    whole.extend_from_slice(prior);
    whole.extend_from_slice(new);
    let corrected = apply_regrades(&whole);
    let fresh = new
        .iter()
        .filter(|event| !matches!(event, Event::Regraded(_)))
        .count();
    let split = corrected.len().saturating_sub(fresh);

    let mut proj = Projector::new(input.graph, input.cfg)
        .with_timezone(input.tz)?
        .with_goal(input.goal)
        .with_cached_topics(cached.topics.clone());
    for (position, event) in corrected.iter().enumerate() {
        proj.apply(event, position >= split);
    }
    proj.finalize(input.now)
}

// --------------------------------------------------------------------------- //
// The parity blob
// --------------------------------------------------------------------------- //

/// The bytes the parity oracle compares (spec section 9).
///
/// Canonical JSON of the model with `built_from_ts` REMOVED, because that field alone
/// carries wall clock (trap T10): sorted keys, compact separators, non-ASCII text
/// unescaped, and NO trailing newline. 1.0 builds the same bytes with
/// `json.dumps(payload, sort_keys=True, separators=(",",":"), ensure_ascii=False)`.
///
/// A float takes the Python `repr` text through [`crate::curriculum::python_repr_f64`], not the
/// `serde_json` text: the two spell an exponent differently, so `1e-05` reads `1e-5`
/// there and the digest diverges. A `serde_json` map is a `BTreeMap`, so its keys
/// come out in Rust `String` order, which is byte order, which equals the Python
/// code-point order for UTF-8 (trap T18).
///
/// # Errors
///
/// Returns [`ProjectorError::Serialize`] when the model does not serialize, which
/// happens only for a timestamp outside the representable range.
pub fn canonical_blob(model: &LearnerModel) -> Result<String, ProjectorError> {
    let mut value = serde_json::to_value(model).map_err(serialize_error)?;
    let _built_from_ts = value
        .as_object_mut()
        .and_then(|object| object.remove("built_from_ts"));
    let mut out = String::new();
    render_json(&value, &mut out);
    Ok(out)
}

/// The lowercase hex SHA-256 of [`canonical_blob`].
///
/// # Errors
///
/// Returns [`ProjectorError::Serialize`] when the blob does not build.
pub fn blob_digest(model: &LearnerModel) -> Result<String, ProjectorError> {
    Ok(sha256_hex(canonical_blob(model)?.as_bytes()))
}

// --------------------------------------------------------------------------- //
// The lesson knowledge-point gates (`projector.py:860-872`)
// --------------------------------------------------------------------------- //

/// Whether a lesson knowledge point is mastered (`projector.py:860-867`).
///
/// The rule is `lesson.kp_pass = "2consec|3of4"`: two correct answers in a row at
/// the tail, or three correct out of the first four. 1.0 hard-codes both arms and
/// reads the config string for neither, so this port takes no config either. A
/// second rule spelling would need a parser in both tiers, and 1.0 has none.
///
/// `seq` is the answer sequence of ONE knowledge point, oldest first.
#[must_use]
pub fn kp_passed(seq: &[bool]) -> bool {
    let len = seq.len();
    if len >= 2 && seq[len - 1] && seq[len - 2] {
        return true;
    }
    if len >= 4 && seq.iter().take(4).filter(|correct| **correct).count() >= 3 {
        return true;
    }
    false
}

/// Whether a lesson knowledge point failed (`projector.py:870-872`).
///
/// A knowledge point fails when `lesson.fail_after` answers stand and
/// [`kp_passed`] is still false.
#[must_use]
pub fn kp_failed(seq: &[bool], cfg: &Config) -> bool {
    i64::try_from(seq.len()).unwrap_or(i64::MAX) >= cfg.lesson.fail_after && !kp_passed(seq)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{stream, tree};
    use super::*;
    use crate::numeric::TimeError;

    #[test]
    fn the_entry_points_fold_the_stream_and_report_an_unknown_zone() {
        let tree = tree();
        let cfg = Config::default();
        let now = Timestamp::from_micros(1_784_031_400_000_000);
        let events = stream();
        let input = ProjectionInput::new(&tree, &cfg, now).with_goal(40);
        let full = project(&events, &input).expect("the fold succeeds");
        let (prior, fresh) = events.split_at(4);
        let cached = project(prior, &input).expect("the prefix folds");
        let resumed = project_incremental(&cached, prior, fresh, &input).expect("the resume folds");
        assert_eq!(
            blob_digest(&resumed).expect("a digest"),
            blob_digest(&full).expect("a digest")
        );
        let blob = canonical_blob(&full).expect("a blob");
        assert!(blob.starts_with("{\"config_hash\":\"") && !blob.contains("built_from_ts"));

        let nowhere = input.with_timezone(Some("Nowhere/City"));
        let unknown = ProjectorError::Time(TimeError::UnknownTimezone("Nowhere/City".to_owned()));
        assert_eq!(project(&events, &nowhere).unwrap_err(), unknown);
        assert!(project_incremental(&cached, prior, fresh, &nowhere).is_err());

        let mut far = full;
        far.built_from_ts = Some(Timestamp::from_micros(i64::MAX));
        let error = canonical_blob(&far).unwrap_err().to_string();
        assert!(error.starts_with("serialize:"), "{error}");
        assert!(blob_digest(&far).is_err());
    }

    #[test]
    fn the_knowledge_point_gates_read_the_tail_and_the_first_four() {
        let cfg = Config::default();
        assert!(kp_passed(&[false, true, true]));
        assert!(kp_passed(&[true, false, true, true, false]));
        assert!(!kp_passed(&[true, false]));
        assert!(kp_failed(&[false, false, false, false, false], &cfg));
        assert!(!kp_failed(&[true, true], &cfg));
    }
}
