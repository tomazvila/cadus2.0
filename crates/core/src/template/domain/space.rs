//! The declared space, the satisfying walk, and the small number helpers.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::One;
use serde::{Deserialize, Serialize};

use super::{Bindings, Domain, DomainError, EXHAUSTIVE_SPACE_LIMIT};
use crate::template::constraint::{Constraint, all_hold};
use crate::template::gate::{GATE_DRAW_BUDGET, GATE_SAMPLES, GATE_SEED};

/// The declared parameters of a template document, in name order.
pub type Params = BTreeMap<String, Domain>;

/// The count of satisfying tuples of a document (spec section 2.3, decision 1).
///
/// The count is the count of tuples the constraints accept, and never the product
/// of the domain sizes. 1.0 stores the product and saturates it at
/// `MAX_DOMAIN_SIZE`, so a 1.0 `space_size` of 10,000 reads "at least 10,000"
/// (spec section 8, trap 9). 2.0 stores an exact count, or the count one walk
/// found with the draws it spent, so the number never lies about what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpaceSize {
    /// The exact count. The walk visited every tuple.
    Exact(u64),
    /// The count the sampled walk found, with the evidence it rests on.
    ///
    /// The count is a FLOOR and never a scaled guess: it counts the DISTINCT
    /// satisfying tuples one walk saw. The walk stops at
    /// [`GATE_SAMPLES`](super::gate::GATE_SAMPLES) distinct tuples or at
    /// [`GATE_DRAW_BUDGET`](super::gate::GATE_DRAW_BUDGET) draws, so a space
    /// larger than `GATE_SAMPLES` reads as `GATE_SAMPLES`.
    Estimated {
        /// The count of DISTINCT satisfying tuples the walk found.
        estimate: u64,
        /// The count of tuples the walk drew.
        samples: u32,
        /// The count of drawn tuples the constraints accepted, repeats included.
        hits: u32,
    },
}

impl SpaceSize {
    /// The count of satisfying tuples, exact or estimated.
    #[must_use]
    pub const fn count(self) -> u64 {
        match self {
            Self::Exact(count) => count,
            Self::Estimated { estimate, .. } => estimate,
        }
    }

    /// Whether the count is exact.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }
}

/// The product of the declared domain sizes, saturating at [`u64::MAX`].
///
/// # Errors
///
/// Returns [`DomainError`] when one domain is empty or past its bound.
pub fn declared_space(params: &Params) -> Result<u64, DomainError> {
    let mut product: u64 = 1;
    for (name, domain) in params {
        product = product.saturating_mul(domain.size(name)?);
    }
    Ok(product)
}

/// Every tuple of the declared domains, in lexicographic order of the names.
///
/// # Errors
///
/// Returns [`DomainError`] when one domain is empty, past its bound, or when the
/// product of the domain sizes is past `limit`.
pub fn enumerate(params: &Params, limit: u64) -> Result<Vec<Bindings>, DomainError> {
    let mut tuples: Vec<Bindings> = vec![Bindings::new()];
    for (name, domain) in params {
        let values = domain.values(name)?;
        let width = u64::try_from(values.len()).unwrap_or(u64::MAX);
        let grown = u64::try_from(tuples.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(width);
        if grown > limit {
            return Err(DomainError::TooLarge {
                name: name.clone(),
                count: grown,
            });
        }
        let mut next = Vec::with_capacity(grown as usize);
        for tuple in &tuples {
            for value in &values {
                let mut extended = tuple.clone();
                extended.insert(name.clone(), value.clone());
                next.push(extended);
            }
        }
        tuples = next;
    }
    Ok(tuples)
}

/// The satisfying tuples of a document, and the count the gate stores.
///
/// The walk is the ONE place 2.0 counts a constrained space. The gate reads its
/// tuples and its count from the same call, so the number a reviewer approves
/// and the instances the gate checked can never disagree (M4 review 2, findings
/// 2 and 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SatisfyingWalk {
    /// The satisfying count: exact below the limit, the found count above it.
    pub space: SpaceSize,
    /// The distinct satisfying tuples, in walk order.
    pub tuples: Vec<Bindings>,
    /// True when `tuples` holds every satisfying tuple of the declared domains.
    pub exhaustive: bool,
    /// The count of tuples the sampled walk drew, and `None` below the limit.
    pub drawn: Option<u32>,
}

/// Walk the space and collect the tuples the constraints accept.
///
/// At or under [`EXHAUSTIVE_SPACE_LIMIT`] declared tuples the walk visits every
/// tuple and the count is exact. Above the limit the walk draws from the seed
/// [`GATE_SEED`](super::gate::GATE_SEED) until it holds
/// [`GATE_SAMPLES`](super::gate::GATE_SAMPLES) distinct satisfying tuples or it
/// spends [`GATE_DRAW_BUDGET`](super::gate::GATE_DRAW_BUDGET) draws, and the
/// count is the count of those distinct tuples.
///
/// # Errors
///
/// Returns [`DomainError`] when a domain is empty or past its bound, and when a
/// constraint cannot decide on a tuple.
pub fn walk_satisfying(
    params: &Params,
    constraints: &[Constraint],
) -> Result<SatisfyingWalk, DomainError> {
    let every_tuple = match enumerate(params, EXHAUSTIVE_SPACE_LIMIT) {
        Ok(tuples) => Some(tuples),
        Err(DomainError::TooLarge { .. }) => None,
        Err(other) => return Err(other),
    };
    if let Some(every_tuple) = every_tuple {
        let mut tuples: Vec<Bindings> = Vec::new();
        for tuple in every_tuple {
            if all_hold(constraints, &tuple)? {
                tuples.push(tuple);
            }
        }
        let count = u64::try_from(tuples.len()).unwrap_or(u64::MAX);
        return Ok(SatisfyingWalk {
            space: SpaceSize::Exact(count),
            tuples,
            exhaustive: true,
            drawn: None,
        });
    }
    let plan = crate::template::draw::DrawPlan::new(params)?;
    let mut rng = crate::template::draw::rng_from_seed(GATE_SEED);
    let mut tuples: Vec<Bindings> = Vec::new();
    let mut distinct: BTreeSet<Bindings> = BTreeSet::new();
    let mut drawn: u32 = 0;
    let mut hits: u32 = 0;
    // A refused draw says nothing about the next one, so the walk keeps drawing
    // past a sparse stretch and stops on the budget alone (M4 review 1, findings
    // 7 and 12).
    while u32::try_from(distinct.len()).unwrap_or(u32::MAX) < GATE_SAMPLES
        && drawn < GATE_DRAW_BUDGET
    {
        drawn = drawn.saturating_add(1);
        let tuple = plan.draw(&mut rng);
        if all_hold(constraints, &tuple)? {
            hits = hits.saturating_add(1);
            if distinct.insert(tuple.clone()) {
                tuples.push(tuple);
            }
        }
    }
    let found = u64::try_from(tuples.len()).unwrap_or(u64::MAX);
    Ok(SatisfyingWalk {
        space: SpaceSize::Estimated {
            estimate: found,
            samples: drawn,
            hits,
        },
        tuples,
        exhaustive: false,
        drawn: Some(drawn),
    })
}

/// Count the tuples the constraints accept (spec section 2.3, decision 1).
///
/// The count is the `space` of [`walk_satisfying`]: exact at or under
/// [`EXHAUSTIVE_SPACE_LIMIT`] declared tuples, and the count of distinct
/// satisfying tuples the sampled walk found above it.
///
/// # Errors
///
/// Returns [`DomainError`] when a domain is empty or past its bound, and when a
/// constraint cannot decide on a tuple.
pub fn space_size(params: &Params, constraints: &[Constraint]) -> Result<SpaceSize, DomainError> {
    Ok(walk_satisfying(params, constraints)?.space)
}

/// The greatest common divisor of two whole numbers. `BigInt::gcd` is never negative.
#[must_use]
pub fn gcd_of(left: &BigInt, right: &BigInt) -> BigInt {
    left.gcd(right)
}

/// Whether the rational is a whole number.
#[must_use]
pub fn is_whole(number: &BigRational) -> bool {
    number.denom().is_one()
}
