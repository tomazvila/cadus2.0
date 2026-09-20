//! The `function` contract: two formulas are equal if they agree at fixed points.
//!
//! This file is the one exception to rule D6 ("never a float"). It compares
//! `f64` values at eight fixed sample points with the tolerance [`TOLERANCE`].
//! It uses no random source and no clock, so one input gives one verdict.

use std::collections::{BTreeMap, BTreeSet};

use num_traits::ToPrimitive;

use crate::answer::{
    Ast, Canon, Outcome, Undecidable, Verdict, canon, canonical_form, normalize, parse,
};

use crate::answer::evalf::free_vars;
pub use crate::answer::evalf::{Env, eval};

/// The position of each sample point in its interval, as `(numerator, denominator)`.
pub const SAMPLE_FRACTIONS: [(u32, u32); 8] = [
    (1, 16),
    (3, 16),
    (5, 16),
    (7, 16),
    (9, 16),
    (11, 16),
    (13, 16),
    (15, 16),
];
/// The smallest count of sample points where a key must have a finite value.
pub const MIN_FINITE_POINTS: usize = 6;
/// The largest permitted difference, relative to `max(1, |key value|)`.
pub const TOLERANCE: f64 = 1e-9;
/// The names that have the value 0 with `up_to_constant`.
pub const CONSTANT_NAMES: [&str; 4] = ["C", "c", "K", "k"];
/// The sample interval of a variable that has no `domain` entry.
pub const DEFAULT_DOMAIN: (&str, &str) = ("1/4", "11/4");

const BAD_VARS: &str = "a function contract requires one to three distinct variable names";
const BAD_DOMAIN: &str =
    "a function domain requires two exact rationals with low below high for a listed variable";
const POINT_NEAR_ZERO: &str =
    "a function contract domain must keep each sample point 1/100 or more from zero";
/// The smallest permitted distance of a variable sample value from 0.
const MIN_SAMPLE_MAGNITUDE: f64 = 0.01;
const NOT_ONE_EXPRESSION: &str = "a function answer must be one expression";

/// The checked form of a `function` contract. `domain[i]` belongs to `vars[i]`.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    /// One to three variable names.
    pub vars: Vec<String>,
    /// If true, a difference that is one constant is correct.
    pub up_to_constant: bool,
    /// The sample interval `(low, high)` of each variable.
    pub domain: Vec<(f64, f64)>,
}

impl FunctionSpec {
    /// Read the fields of a `function` contract.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] for a variable list or a domain outside the rules.
    pub fn new(
        vars: &[String],
        up_to_constant: bool,
        domain: &BTreeMap<String, (String, String)>,
    ) -> Result<Self, Undecidable> {
        let names: BTreeSet<&String> = vars.iter().collect();
        let listed = (1..=3).contains(&vars.len())
            && names.len() == vars.len()
            && vars.iter().all(|name| one_variable(name));
        if !listed {
            return Err(Undecidable::new(BAD_VARS));
        }
        if domain.keys().any(|name| !names.contains(name)) {
            return Err(Undecidable::new(BAD_DOMAIN));
        }
        let domain = vars
            .iter()
            .map(|name| interval(domain.get(name)))
            .collect::<Result<Vec<_>, _>>()?;
        let spec = Self {
            vars: vars.to_vec(),
            up_to_constant,
            domain,
        };
        // A point near 0 makes a key such as `(1 - cos(x))/x^2` lose its digits,
        // and an equal formula then gets "wrong".
        let near_zero = spec.sample_points().iter().any(|env| {
            vars.iter()
                .any(|name| env[name].abs() < MIN_SAMPLE_MAGNITUDE)
        });
        if near_zero {
            return Err(Undecidable::new(POINT_NEAR_ZERO));
        }
        Ok(spec)
    }

    /// The eight sample points. Each point has a value for each variable.
    ///
    /// Point `j` gives variable `i` the fraction `(j * (2 * i + 1) + i) % 8` of
    /// its interval, so no two variables have the same fraction at one point.
    #[must_use]
    pub fn sample_points(&self) -> Vec<Env> {
        (0..SAMPLE_FRACTIONS.len())
            .map(|point| self.sample_point(point))
            .collect()
    }

    fn sample_point(&self, point: usize) -> Env {
        let mut env = Env::new();
        if self.up_to_constant {
            for name in CONSTANT_NAMES {
                env.insert(name.to_string(), 0.0);
            }
        }
        for (index, (name, (low, high))) in self.vars.iter().zip(&self.domain).enumerate() {
            let (numerator, denominator) =
                SAMPLE_FRACTIONS[(point * (2 * index + 1) + index) % SAMPLE_FRACTIONS.len()];
            let fraction = f64::from(numerator) / f64::from(denominator);
            env.insert(name.clone(), low + (high - low) * fraction);
        }
        env
    }

    /// Whether a formula of this contract can use the name.
    fn permits(&self, name: &str) -> bool {
        self.vars.iter().any(|listed| listed == name)
            || (self.up_to_constant && CONSTANT_NAMES.contains(&name))
    }

    fn permits_each_name(&self, tree: &Ast) -> bool {
        free_vars(tree).iter().all(|name| self.permits(name))
    }
}

/// Whether the parser reads the name as one variable (`e` and `pi` are constants).
fn one_variable(name: &str) -> bool {
    matches!(parse(&normalize(name).source), Ok(Ast::Var(read)) if read == name)
}

fn interval(bounds: Option<&(String, String)>) -> Result<(f64, f64), Undecidable> {
    let (low, high) = bounds.map_or(DEFAULT_DOMAIN, |(low, high)| (low.as_str(), high.as_str()));
    match (rational(low), rational(high)) {
        (Some(low), Some(high)) if low < high && (high - low).is_finite() => Ok((low, high)),
        _ => Err(Undecidable::new(BAD_DOMAIN)),
    }
}

fn rational(text: &str) -> Option<f64> {
    match canonical_form(text) {
        Ok(Canon::Rational(value)) => value.to_f64(),
        _ => None,
    }
}

/// Validate an authored key. A leading `name =` label is permitted.
///
/// # Errors
///
/// Returns [`Undecidable`] if the parser refuses the key, if the key is not one
/// expression, if it uses a name outside its variables, if it uses `log` or the
/// e notation of a number (D28), or if it has a finite value at fewer than
/// [`MIN_FINITE_POINTS`] sample points.
pub fn expected(spec: &FunctionSpec, expected: &str) -> Result<Canon, Undecidable> {
    let normalized = normalize(expected);
    if let Some(reason) = ambiguous_notation(&normalized.source) {
        return Err(Undecidable::new(reason));
    }
    let tree = parse(&normalized.source)?;
    let formula = body(&tree);
    if !scalar(formula) {
        return Err(Undecidable::new(
            "the authored function must be one expression",
        ));
    }
    if !spec.permits_each_name(formula) {
        return Err(Undecidable::new(
            "the authored function uses a name outside its variables",
        ));
    }
    let points = spec.sample_points();
    let finite = points.iter().filter_map(|env| eval(formula, env)).count();
    if finite < MIN_FINITE_POINTS {
        return Err(Undecidable::new(
            "the authored function has fewer than six finite sample points",
        ));
    }
    Ok(canon(&tree).unwrap_or(Canon::Label(normalized.string_key)))
}

/// The notation that a key must not use, because it has two readings (D28).
///
/// `log` is the natural logarithm for the evaluator and base 10 for the
/// curriculum. The parser reads `1e-5` as `1*e - 5`, not as a power of ten.
fn ambiguous_notation(source: &str) -> Option<&'static str> {
    if source.to_lowercase().contains("log") {
        return Some("the authored function must use ln, because log has two readings");
    }
    let bytes = source.as_bytes();
    let exponent_at = |at: usize| {
        let sign = usize::from(matches!(bytes.get(at + 2), Some(b'+' | b'-')));
        bytes[at].is_ascii_digit()
            && matches!(bytes.get(at + 1), Some(b'e' | b'E'))
            && bytes.get(at + 2 + sign).is_some_and(u8::is_ascii_digit)
    };
    (0..bytes.len())
        .any(exponent_at)
        .then_some("the authored function must use 10^(n), because the form 1e-5 reads as 1*e - 5")
}

/// Grade a learner text against an authored key.
#[must_use]
pub fn check(spec: &FunctionSpec, expected: &str, learner: &str) -> Outcome {
    let trees = parse(&normalize(expected).source)
        .and_then(|key| parse(&normalize(learner).source).map(|answer| (key, answer)));
    match trees {
        Ok((key, answer)) if scalar(body(&answer)) => Outcome::Decided(grade(&key, &answer, spec)),
        Ok(_) => Outcome::Undecidable(Undecidable::new(NOT_ONE_EXPRESSION)),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

/// Compare two formulas. A leading label on the two trees has no effect.
///
/// The steps: (1) equal canonical forms are correct; (2) a learner name outside
/// the variables is wrong; (3) a learner value that is not finite where the key
/// is finite is wrong; (4) each difference must be in the tolerance; (5) with
/// `up_to_constant`, the spread of the differences must be in the tolerance.
/// A key with fewer than [`MIN_FINITE_POINTS`] finite points gives "wrong".
#[must_use]
pub fn grade(expected: &Ast, learner: &Ast, spec: &FunctionSpec) -> Verdict {
    let (expected, learner) = (body(expected), body(learner));
    let same_form =
        matches!((canon(expected), canon(learner)), (Ok(key), Ok(answer)) if key == answer);
    Verdict {
        correct: same_form || (spec.permits_each_name(learner) && agrees(expected, learner, spec)),
        notation: false,
    }
}

fn agrees(expected: &Ast, learner: &Ast, spec: &FunctionSpec) -> bool {
    let mut pairs = Vec::new();
    for env in spec.sample_points() {
        let Some(key) = eval(expected, &env) else {
            continue;
        };
        let Some(answer) = eval(learner, &env) else {
            return false;
        };
        pairs.push((key, answer));
    }
    if pairs.len() < MIN_FINITE_POINTS {
        return false;
    }
    if spec.up_to_constant {
        constant_difference(&pairs)
    } else {
        pairs
            .iter()
            .all(|(key, answer)| (key - answer).abs() <= TOLERANCE * key.abs().max(1.0))
    }
}

/// Whether `key - answer` is one constant across the points.
fn constant_difference(pairs: &[(f64, f64)]) -> bool {
    let scale = pairs
        .iter()
        .fold(1.0_f64, |scale, (key, _)| scale.max(key.abs()));
    let (low, high) = pairs
        .iter()
        .map(|(key, answer)| key - answer)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| {
            (low.min(value), high.max(value))
        });
    high - low <= TOLERANCE * scale
}

/// The formula under one leading label, or the tree itself.
fn body(tree: &Ast) -> &Ast {
    match tree {
        Ast::Assign { value, .. } => value,
        other => other,
    }
}

/// Whether the tree is one expression with a real value.
fn scalar(tree: &Ast) -> bool {
    match tree {
        Ast::Var(_)
        | Ast::Const(_)
        | Ast::Integer(_)
        | Ast::Fraction { .. }
        | Ast::Mixed { .. }
        | Ast::Decimal { .. } => true,
        Ast::Sqrt(inner) | Ast::Neg(inner) | Ast::Pow(inner, _) => scalar(inner),
        Ast::RationalPow { base, .. } => scalar(base),
        Ast::Add(items) | Ast::Mul(items) | Ast::Func(_, items) => items.iter().all(scalar),
        Ast::Div(top, bottom) => scalar(top) && scalar(bottom),
        Ast::Assign { .. }
        | Ast::Chain { .. }
        | Ast::Ineq { .. }
        | Ast::Interval { .. }
        | Ast::List(_)
        | Ast::Quantity { .. }
        | Ast::Set(_)
        | Ast::Tuple(_) => false,
    }
}
