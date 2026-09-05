//! Seeded parameter draws (A5, D-O4, spec section 3.1 and 3.2).
//!
//! # The generator
//!
//! 1.0 draws from `random.Random`, the Mersenne Twister, and the serve path
//! constructs it unseeded, so it takes operating-system entropy
//! (`problem_templates.py:1296`). Nobody can reproduce a served instance from the
//! record. 2.0 draws from `ChaCha8Rng` seeded with a recorded `u64`
//! (`docs/plans/M4.md`, the fixed RNG decision). Two properties follow: a reviewer
//! reproduces any served instance from the seed and the document, and the L1
//! benchmark is deterministic.
//!
//! The core never reads a clock and never reads operating-system entropy. The
//! caller of the refill job picks the seed and records it on the pool row.
//!
//! # The bounded draw
//!
//! [`below`] draws a whole number below a bound with Lemire's multiply-and-shift
//! method. The naive `next_u64() % range` is biased: with a range that does not
//! divide 2^64, the low values of the range come out more often, and the bias
//! lands exactly on the low end of an integer domain — the end the gate calls
//! "where an expression stops being right". The one `%` this method runs is on
//! the rejection threshold, which depends on the bound alone and never on a
//! drawn value.
//!
//! # The plan
//!
//! [`DrawPlan`] materializes the value list of every parameter ONCE, the way 1.0
//! materializes `domain.values()`. A refill batch builds one plan and draws from
//! it many times, so a draw allocates the bound tuple and nothing else. Every
//! draw is uniform over the DISTINCT values of the domain, which is what lets
//! the sampled walk of `space_size` reach every satisfying tuple the exhaustive
//! walk visits.
//!
//! # The candidate stream
//!
//! [`candidates`] keeps the 1.0 split (`problem_templates.py:353-375`):
//!
//! - At or under [`EXHAUSTIVE_SPACE_LIMIT`] declared tuples, build the whole
//!   product, shuffle it, and yield every satisfying tuple once. Avoidance is
//!   then exact: if an unblocked instance exists, the walk reaches it.
//! - Above the limit, yield at most [`RESAMPLE_ATTEMPTS`] independent draws. 1.0
//!   states the reason at `:211-216`: with 12 values and 11 already served, 24
//!   random draws miss the free value about 13% of the time, so the small space
//!   needs the exhaustive walk. A draw that spends its [`MAX_REJECTIONS`] budget
//!   is skipped and the stream goes on, so a sparse constraint set costs draws
//!   and never an empty stream.
//!
//! A constrained draw NEVER returns a tuple the constraints refuse. It returns
//! [`DrawError::NoSatisfyingTuple`] instead, so a violating tuple cannot reach a
//! learner.

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use super::constraint::{Constraint, ConstraintError, all_hold};
use std::collections::BTreeMap;

use super::domain::{Bindings, DomainError, EXHAUSTIVE_SPACE_LIMIT, Params, Value, enumerate};

/// The count of independent draws above [`EXHAUSTIVE_SPACE_LIMIT`] (1.0 `RESAMPLE_ATTEMPTS`).
pub const RESAMPLE_ATTEMPTS: usize = 24;

/// The count of draws [`DrawPlan::draw_satisfying`] spends on one tuple.
///
/// The bound turns a constraint set that almost never holds into an error, and
/// not into a loop. The refill job (D-O4) reads the error and flags the template;
/// the serve path never runs this loop (L1).
pub const MAX_REJECTIONS: usize = 1_000;

/// A draw the instantiator refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DrawError {
    /// A domain is empty, past its bound, or holds a zero denominator.
    #[error("{0}")]
    Domain(#[from] DomainError),
    /// A constraint cannot decide on a drawn tuple.
    #[error("{0}")]
    Constraint(#[from] ConstraintError),
    /// No tuple of the declared domains satisfies the constraints.
    #[error("no tuple of the declared domains satisfies the constraints after {attempts} draw(s)")]
    NoSatisfyingTuple {
        /// The count of tuples the draw tried.
        attempts: usize,
    },
}

/// Build the generator of one refill batch from its recorded seed.
#[must_use]
pub fn rng_from_seed(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// Draw a whole number below `bound`, with no modulo bias.
///
/// The method is Lemire's multiply-and-shift with a rejection threshold. A bound
/// of zero or one has one answer, zero, so the draw reads no value for it.
pub fn below(rng: &mut ChaCha8Rng, bound: u64) -> u64 {
    if bound <= 1 {
        return 0;
    }
    // The threshold depends on the bound alone. It counts the values at the top
    // of the 64-bit range that make the fold uneven: `2**64 mod bound`.
    let threshold = u64::MAX.wrapping_sub(bound).wrapping_add(1) % bound;
    let mut drawn = u128::from(rng.next_u64()) * u128::from(bound);
    while (drawn as u64) < threshold {
        drawn = u128::from(rng.next_u64()) * u128::from(bound);
    }
    (drawn >> 64) as u64
}

/// The materialized value list of every declared parameter.
///
/// The plan is built once per refill batch. A draw from it allocates the bound
/// tuple and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawPlan {
    /// One column per parameter, in name order: the name and its distinct values.
    columns: Vec<(String, Vec<Value>)>,
}

impl DrawPlan {
    /// Build the plan of a parameter set.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] when a domain is empty, past [`super::domain::MAX_DOMAIN_SIZE`],
    /// or when a rational domain holds a zero denominator.
    pub fn new(params: &Params) -> Result<Self, DomainError> {
        let mut columns = Vec::with_capacity(params.len());
        for (name, domain) in params {
            columns.push((name.clone(), domain.values(name)?));
        }
        Ok(Self { columns })
    }

    /// The product of the distinct value counts, saturating at [`u64::MAX`].
    #[must_use]
    pub fn declared_space(&self) -> u64 {
        let mut product: u64 = 1;
        for (_, values) in &self.columns {
            product = product.saturating_mul(u64::try_from(values.len()).unwrap_or(u64::MAX));
        }
        product
    }

    /// Build the plan from value lists the caller validated, in name order.
    ///
    /// The gate reads every domain once for its own checks and builds the plan
    /// from those lists, so no domain is read twice.
    #[must_use]
    pub fn from_columns(columns: BTreeMap<String, Vec<Value>>) -> Self {
        Self {
            columns: columns.into_iter().collect(),
        }
    }

    /// Draw one tuple, with no constraint applied.
    ///
    /// Every column draws uniformly over its distinct values.
    #[must_use]
    pub fn draw(&self, rng: &mut ChaCha8Rng) -> Bindings {
        let mut bindings = Bindings::new();
        for (name, values) in &self.columns {
            let width = u64::try_from(values.len()).unwrap_or(u64::MAX);
            let index = usize::try_from(below(rng, width)).unwrap_or(0);
            bindings.extend(values.get(index).map(|value| (name.clone(), value.clone())));
        }
        bindings
    }

    /// Draw one tuple that satisfies every constraint.
    ///
    /// The function never returns a tuple a constraint refuses. It gives up after
    /// [`MAX_REJECTIONS`] draws and reports [`DrawError::NoSatisfyingTuple`].
    ///
    /// # Errors
    ///
    /// Returns [`DrawError`] for an undecidable constraint and for a constraint
    /// set no draw satisfied.
    pub fn draw_satisfying(
        &self,
        constraints: &[Constraint],
        rng: &mut ChaCha8Rng,
    ) -> Result<Bindings, DrawError> {
        for _ in 0..MAX_REJECTIONS {
            let bindings = self.draw(rng);
            if all_hold(constraints, &bindings)? {
                return Ok(bindings);
            }
        }
        Err(DrawError::NoSatisfyingTuple {
            attempts: MAX_REJECTIONS,
        })
    }
}

/// Draw one tuple of the declared domains, with no constraint applied.
///
/// The call builds a [`DrawPlan`] and drops it. A caller that draws more than
/// once builds the plan itself and keeps it.
///
/// # Errors
///
/// Returns [`DomainError`] for every domain [`DrawPlan::new`] refuses.
pub fn draw_bindings(params: &Params, rng: &mut ChaCha8Rng) -> Result<Bindings, DomainError> {
    Ok(DrawPlan::new(params)?.draw(rng))
}

/// Draw one tuple that satisfies every constraint.
///
/// # Errors
///
/// Returns [`DrawError`] for a refused domain, an undecidable constraint, and a
/// constraint set no draw satisfied.
pub fn draw_satisfying(
    params: &Params,
    constraints: &[Constraint],
    rng: &mut ChaCha8Rng,
) -> Result<Bindings, DrawError> {
    DrawPlan::new(params)?.draw_satisfying(constraints, rng)
}

/// Build the candidate stream of one instantiation (spec section 3.1).
///
/// Every tuple of the stream satisfies every constraint. The order is the
/// shuffled product below the limit, and independent draws above it.
///
/// # Errors
///
/// Returns [`DrawError`] for a refused domain and an undecidable constraint. A
/// document whose constraints no tuple satisfies yields an empty stream, and the
/// caller decides what that means.
pub fn candidates(
    params: &Params,
    constraints: &[Constraint],
    rng: &mut ChaCha8Rng,
) -> Result<Vec<Bindings>, DrawError> {
    let every_tuple = match enumerate(params, EXHAUSTIVE_SPACE_LIMIT) {
        Ok(tuples) => tuples,
        Err(DomainError::TooLarge { .. }) => return sampled_candidates(params, constraints, rng),
        Err(other) => return Err(DrawError::Domain(other)),
    };
    let mut satisfying = Vec::new();
    for tuple in every_tuple {
        if all_hold(constraints, &tuple)? {
            satisfying.push(tuple);
        }
    }
    shuffle(&mut satisfying, rng);
    Ok(satisfying)
}

/// The candidate stream above the exhaustive limit: independent draws.
fn sampled_candidates(
    params: &Params,
    constraints: &[Constraint],
    rng: &mut ChaCha8Rng,
) -> Result<Vec<Bindings>, DrawError> {
    let plan = DrawPlan::new(params)?;
    let mut out = Vec::with_capacity(RESAMPLE_ATTEMPTS);
    for _ in 0..RESAMPLE_ATTEMPTS {
        match plan.draw_satisfying(constraints, rng) {
            Ok(bindings) => out.push(bindings),
            // A draw that spends its budget is skipped, and the stream goes on.
            // One spent draw says nothing about the next one: with a density of
            // one tuple in a thousand, six draws in ten spend the budget and the
            // stream that stopped there was empty (M4 review 1, findings 7
            // and 12, the same defect in the gate walk).
            Err(DrawError::NoSatisfyingTuple { .. }) => continue,
            Err(other) => return Err(other),
        }
    }
    Ok(out)
}

/// Shuffle a list in place, with the Fisher-Yates walk.
///
/// The walk swaps each position with a position at or below it, and it draws
/// every index through [`below`], so the permutation is uniform and reproducible
/// from the seed.
pub fn shuffle<T>(items: &mut [T], rng: &mut ChaCha8Rng) {
    for at in (1..items.len()).rev() {
        let bound = u64::try_from(at + 1).unwrap_or(1);
        // `pick` is at most `at`, and `at` is below the length.
        let pick = usize::try_from(below(rng, bound)).unwrap_or(0);
        items.swap(at, pick);
    }
}
