//! The anti-repeat state and the candidate rule (A5, D5, D-S6, D-O1).
//!
//! # Three layers, and they are not the same thing
//!
//! Specification section 5.5 names three anti-repeat layers of 2.0. This module
//! owns the second and the third:
//!
//! | Layer | Where it lives | What it stops |
//! |---|---|---|
//! | `UNIQUE (user_id, kp_id, instance_hash)` | the database (`migrations/0005_content.sql`) | the same instance enters the pool twice |
//! | [`Ring`], 20 entries per `(user, topic)` | the D-S6 state row | the same statement comes back inside a topic |
//! | [`TaskMemory`], 12 entries per task | the D-S6 state row | the same statement comes back inside one task |
//!
//! The two sizes are the 1.0 sizes. 20 is `LAST_PROBLEMS_WINDOW`
//! (`cadus/projector.py:92`), and 12 is `SERVED_TEXT_MEMORY`
//! (`cadus_web/state.py:133`).
//!
//! # Hashes, never statements
//!
//! 1.0 keeps the verbatim statement in the per-task memory, because a model read
//! it back in the next prompt (`cadus_web/api.py:492-496`). 2.0 calls no model
//! (T1), so nothing reads the statement back. Both windows keep the 12-hex digest
//! of [`crate::learner::problem_text_hash`] and nothing else. Two properties
//! follow: the D-S6 row stays small, and the `.strip()` asymmetry of specification
//! trap 2 has no spelling here, because one rule now decides both windows.
//!
//! # The ring and the projector window
//!
//! [`Ring`] is not `TopicState::last_problems`. The projector window folds on an
//! `attempt` event and gives the M3 parity streams
//! (`cadus/projector.py:213-224`). The D5 ring folds on a serve and lives in the
//! D-S6 state row, because D5 puts it there. Both hold 20 digests of the same
//! function, so their contents agree while the learner answers every problem the
//! server serves.
//!
//! # The `HashSet` view
//!
//! D5 asks for "a fixed-size ring of recent instance hashes plus a `HashSet`
//! view". [`Avoid`] is that view. The serve path builds it once per request over
//! at most 32 borrowed digests, and the candidate loop queries it. The ring
//! itself stays a plain array on the wire, so the D-S6 document holds no second
//! copy of the same digests. The order of the set never reaches an output: the
//! rule only asks the set a question (L1, and a deterministic serve).
//!
//! # The candidate rule
//!
//! [`pick`] is the rule of specification section 5.4 and of the M4 pool decision:
//! walk the candidates in order, skip every ring hit, and take the first
//! survivor. If every candidate is blocked, take the LAST candidate and report
//! [`Pick::exhausted`]. 1.0 states the reason at `problem_templates.py:388-392`:
//! "A repeat is a far smaller failure than no problem, and the alternative is the
//! model call this module exists to avoid."
//!
//! The rule never draws and never redraws. The redraw belongs to the worker
//! (D-O4), where a miss costs nothing on the request path (L1).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::template::Instance;

use super::PoolCounters;

/// The count of instance hashes the D5 ring keeps per `(user, topic)`.
///
/// 20, the value of 1.0 `LAST_PROBLEMS_WINDOW` (`cadus/projector.py:92`).
pub const RING_CAPACITY: usize = 20;

/// The count of statement hashes the per-task memory keeps.
///
/// 12, the value of 1.0 `SERVED_TEXT_MEMORY` (`cadus_web/state.py:133`).
pub const TASK_MEMORY_CAPACITY: usize = 12;

/// Append one digest and drop the oldest entries above `capacity`.
///
/// The window keeps a duplicate digest, exactly as the 1.0 fold does
/// (`cadus/projector.py:213-224`). A repeat therefore costs two slots, and the
/// window records what the server served, not what it served once.
fn push_bounded(window: &mut Vec<String>, hash: &str, capacity: usize) {
    window.push(hash.to_string());
    keep_newest(window, capacity);
}

/// Truncate a window to its newest `capacity` entries.
fn keep_newest(window: &mut Vec<String>, capacity: usize) {
    if window.len() > capacity {
        let drop = window.len().saturating_sub(capacity);
        window.drain(..drop);
    }
}

/// The D5 anti-repeat ring of one `(user, topic)` pair.
///
/// The document is `{"hashes": [...]}`, oldest first and newest last, with at
/// most [`RING_CAPACITY`] entries. It is a field of the D-S6 state row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ring {
    /// The digests, oldest first.
    #[serde(default)]
    hashes: Vec<String>,
}

impl Ring {
    /// Build an empty ring.
    #[must_use]
    pub const fn new() -> Self {
        Self { hashes: Vec::new() }
    }

    /// The count of digests the ring keeps.
    #[must_use]
    pub const fn capacity() -> usize {
        RING_CAPACITY
    }

    /// Build a ring from digests in age order, oldest first.
    ///
    /// The call keeps the newest [`RING_CAPACITY`] digests and drops the rest, so
    /// a state row that carries an over-long array loads to a legal ring.
    #[must_use]
    pub fn from_hashes<I, S>(hashes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut window: Vec<String> = hashes.into_iter().map(Into::into).collect();
        keep_newest(&mut window, RING_CAPACITY);
        Self { hashes: window }
    }

    /// Record one served instance hash as the newest entry.
    pub fn push(&mut self, hash: &str) {
        push_bounded(&mut self.hashes, hash, RING_CAPACITY);
    }

    /// Whether the ring holds the digest.
    #[must_use]
    pub fn contains(&self, hash: &str) -> bool {
        self.hashes.iter().any(|held| held == hash)
    }

    /// The digests, oldest first.
    #[must_use]
    pub fn hashes(&self) -> &[String] {
        &self.hashes
    }

    /// The count of digests the ring holds now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Whether the ring holds no digest.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    /// Drop every digest.
    pub fn clear(&mut self) {
        self.hashes.clear();
    }
}

/// The per-task memory of one task.
///
/// The document is `{"hashes": [...]}`, oldest first and newest last, with at
/// most [`TASK_MEMORY_CAPACITY`] entries. It is a field of the D-S6 state row,
/// keyed by task id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMemory {
    /// The digests, oldest first.
    #[serde(default)]
    hashes: Vec<String>,
}

impl TaskMemory {
    /// Build an empty memory.
    #[must_use]
    pub const fn new() -> Self {
        Self { hashes: Vec::new() }
    }

    /// The count of digests the memory keeps.
    #[must_use]
    pub const fn capacity() -> usize {
        TASK_MEMORY_CAPACITY
    }

    /// Build a memory from digests in age order, oldest first.
    ///
    /// The call keeps the newest [`TASK_MEMORY_CAPACITY`] digests.
    #[must_use]
    pub fn from_hashes<I, S>(hashes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut window: Vec<String> = hashes.into_iter().map(Into::into).collect();
        keep_newest(&mut window, TASK_MEMORY_CAPACITY);
        Self { hashes: window }
    }

    /// Record one served statement hash as the newest entry.
    pub fn push(&mut self, hash: &str) {
        push_bounded(&mut self.hashes, hash, TASK_MEMORY_CAPACITY);
    }

    /// Whether the memory holds the digest.
    #[must_use]
    pub fn contains(&self, hash: &str) -> bool {
        self.hashes.iter().any(|held| held == hash)
    }

    /// The digests, oldest first.
    #[must_use]
    pub fn hashes(&self) -> &[String] {
        &self.hashes
    }

    /// The count of digests the memory holds now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Whether the memory holds no digest.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    /// Drop every digest.
    pub fn clear(&mut self) {
        self.hashes.clear();
    }
}

/// The blocked-digest view of one serve (D5).
///
/// The view borrows the digests of a [`Ring`] and of a [`TaskMemory`]. It copies
/// no string, and the serve path builds it once per request.
#[derive(Debug, Clone, Default)]
pub struct Avoid<'state> {
    /// Every digest the serve refuses.
    blocked: HashSet<&'state str>,
}

impl<'state> Avoid<'state> {
    /// Build the view of one topic ring and one task memory.
    #[must_use]
    pub fn new(ring: &'state Ring, task: &'state TaskMemory) -> Self {
        let mut blocked =
            HashSet::with_capacity(RING_CAPACITY.saturating_add(TASK_MEMORY_CAPACITY));
        for hash in ring.hashes() {
            blocked.insert(hash.as_str());
        }
        for hash in task.hashes() {
            blocked.insert(hash.as_str());
        }
        Self { blocked }
    }

    /// Build the view of one topic ring alone.
    #[must_use]
    pub fn from_ring(ring: &'state Ring) -> Self {
        let mut blocked = HashSet::with_capacity(RING_CAPACITY);
        for hash in ring.hashes() {
            blocked.insert(hash.as_str());
        }
        Self { blocked }
    }

    /// Build a view that blocks nothing.
    #[must_use]
    pub fn none() -> Self {
        Self {
            blocked: HashSet::new(),
        }
    }

    /// Whether the view blocks the digest.
    #[must_use]
    pub fn blocks(&self, hash: &str) -> bool {
        self.blocked.contains(hash)
    }

    /// The count of distinct digests the view blocks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocked.len()
    }

    /// Whether the view blocks no digest.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocked.is_empty()
    }
}

/// Anything the candidate rule reads a digest from.
///
/// The trait is the seam between this rule and the storage row of U4: the store
/// implements it on its pool row, so the rule runs on the popped rows and copies
/// no digest.
pub trait Candidate {
    /// The digest of the rendered statement.
    fn instance_hash(&self) -> &str;
}

impl Candidate for Instance {
    fn instance_hash(&self) -> &str {
        &self.instance_hash
    }
}

impl Candidate for String {
    fn instance_hash(&self) -> &str {
        self.as_str()
    }
}

impl Candidate for &str {
    fn instance_hash(&self) -> &str {
        self
    }
}

/// The outcome of the candidate rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    /// The position of the chosen candidate in the candidate list.
    pub index: usize,
    /// The count of blocked candidates the rule walked past.
    pub skipped: usize,
    /// Whether every candidate was blocked, so the rule chose a repeat.
    pub exhausted: bool,
}

/// Choose one candidate: skip every ring hit, take the first survivor.
///
/// If every candidate is blocked, the rule takes the LAST candidate and sets
/// [`Pick::exhausted`]. An empty candidate list returns `None`, because there is
/// nothing to serve and the caller decides what that means (specification
/// section 7.2: the serve path instantiates an exemplar and raises the A6 flag).
#[must_use]
pub fn pick<C: Candidate>(candidates: &[C], avoid: &Avoid<'_>) -> Option<Pick> {
    if candidates.is_empty() {
        return None;
    }
    for (index, candidate) in candidates.iter().enumerate() {
        if !avoid.blocks(candidate.instance_hash()) {
            return Some(Pick {
                index,
                skipped: index,
                exhausted: false,
            });
        }
    }
    Some(Pick {
        index: candidates.len().saturating_sub(1),
        skipped: candidates.len(),
        exhausted: true,
    })
}

/// Run the candidate rule, record the served digest, and count the outcome.
///
/// The call does the whole in-memory half of the D-O1 serve: it picks, it writes
/// the digest into the topic ring and into the task memory, and it counts the
/// serve. The caller writes the two documents back to the D-S6 row and claims
/// the pool row in the same transaction (specification section 7.2).
///
/// The call runs no draw, opens no socket, and reads no clock (T1, L1, R3).
pub fn serve<'pool, C: Candidate>(
    candidates: &'pool [C],
    ring: &mut Ring,
    task: &mut TaskMemory,
    counters: &mut PoolCounters,
) -> Option<&'pool C> {
    let chosen = {
        let avoid = Avoid::new(ring, task);
        pick(candidates, &avoid)?
    };
    let served = candidates.get(chosen.index)?;
    let hash = served.instance_hash().to_string();
    ring.push(&hash);
    task.push(&hash);
    counters.record(chosen);
    Some(served)
}
