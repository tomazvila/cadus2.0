//! The `function` contract: two formulas are equal if they agree at fixed points.
//!
//! This file is the one exception to rule D6 ("never a float"): it compares f64
//! values at eight fixed sample points with the tolerance [`TOLERANCE`], with no
//! random source and no clock, so one input gives one verdict.

use std::collections::{BTreeMap, BTreeSet};

use num_traits::ToPrimitive;

pub use crate::answer::evalf::{Env, eval};
use crate::answer::{
    Ast, Canon, Outcome, Undecidable, Verdict, canon, canonical_form, evalf::free_vars, normalize,
    parse,
};

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
/// The symmetric interval that such a variable tries first (D45).
const SYMMETRIC_DEFAULT: (f64, f64) = (-3.0, 3.0);

const BAD_VARS: &str = "a function contract requires one to three distinct variable names";
const BAD_DOMAIN: &str =
    "a function domain requires two exact rationals with low below high for a listed variable";
const POINT_NEAR_ZERO: &str =
    "a function contract domain must keep each sample point 1/100 or more from zero";
/// The smallest permitted distance of a variable sample value from 0.
const MIN_SAMPLE_MAGNITUDE: f64 = 0.01;
const NOT_ONE_EXPRESSION: &str = "a function answer must be one expression";
const NOT_ADDITIVE: &str =
    "the constant of the authored function is not additive; put it into vars";
const UNTESTED_ABS: &str = "the domain of the authored function does not test its absolute value";
const QUARTER_POINTS: &str =
    "the domain of the authored function puts each sample point on a quarter";

/// The checked form of a `function` contract. `domain[i]` belongs to `vars[i]`.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    /// One to three variable names.
    pub vars: Vec<String>,
    /// If true, a difference that is one constant is correct.
    pub up_to_constant: bool,
    /// The sample interval `(low, high)` of each variable.
    pub domain: Vec<(f64, f64)>,
    /// True when the interval of the variable came from a default (D45).
    defaulted: Vec<bool>,
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
        let defaulted = vars.iter().map(|name| !domain.contains_key(name)).collect();
        let domain = vars
            .iter()
            .map(|name| interval(domain.get(name)))
            .collect::<Result<Vec<_>, _>>()?;
        let spec = Self {
            vars: vars.to_vec(),
            up_to_constant,
            domain,
            defaulted,
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

    /// The eight sample points. Point `j` gives variable `i` the fraction
    /// `(j * (2 * i + 1) + i) % 8` of its interval, so no two variables have
    /// the same fraction at one point.
    #[must_use]
    pub fn sample_points(&self) -> Vec<Env> {
        self.points_with_constants(0.0)
    }

    /// The eight sample points, with each constant name at `constant` (D34).
    fn points_with_constants(&self, constant: f64) -> Vec<Env> {
        (0..SAMPLE_FRACTIONS.len())
            .map(|point| self.sample_point(point, constant))
            .collect()
    }

    fn sample_point(&self, point: usize, constant: f64) -> Env {
        let mut env = Env::new();
        if self.up_to_constant {
            for name in CONSTANT_NAMES {
                env.insert(name.to_string(), constant);
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

    /// The spec that a key is validated and graded on (D45): each variable
    /// without an explicit domain tries the symmetric default first; the
    /// positive default stays when the key is finite at fewer than
    /// [`MIN_FINITE_POINTS`] of the 8 points. The choice reads the key only.
    fn for_key(&self, key: &Ast) -> FunctionSpec {
        let mut spec = self.clone();
        for index in 0..spec.vars.len() {
            if !spec.defaulted[index] {
                continue;
            }
            let fallback = spec.domain[index];
            spec.domain[index] = SYMMETRIC_DEFAULT;
            if spec.finite_points(key) < MIN_FINITE_POINTS {
                spec.domain[index] = fallback;
            }
        }
        spec
    }

    /// The count of sample points where the key has a finite value.
    fn finite_points(&self, key: &Ast) -> usize {
        self.points_with_constants(0.0)
            .iter()
            .filter(|env| eval(key, env).is_some())
            .count()
    }

    /// Whether `key(C = 1) - key(C = 0)` is one value over the sample points
    /// (D44): the constant of the key is one additive term.
    fn constant_is_additive(&self, key: &Ast) -> bool {
        let mut pairs = Vec::new();
        for (low, high) in self
            .points_with_constants(0.0)
            .iter()
            .zip(self.points_with_constants(1.0))
        {
            if let (Some(value), Some(shifted)) = (eval(key, low), eval(key, &high)) {
                pairs.push((value, shifted));
            }
        }
        let Some(&(first_value, first_shifted)) = pairs.first() else {
            return true;
        };
        pairs.iter().all(|(value, shifted)| {
            let scale = value
                .abs()
                .max(first_value.abs())
                .max(shifted.abs())
                .max(1.0);
            let scale = scale.max(first_shifted.abs());
            ((shifted - value) - (first_shifted - first_value)).abs() <= TOLERANCE * scale
        })
    }

    /// Whether the default domain keeps one `abs` argument of the key at one
    /// sign (D45). The check reads only arguments on a variable of the
    /// default; a written domain is the choice of the author.
    fn hides_a_sign(&self, key: &Ast) -> bool {
        if !self.defaulted.iter().any(|defaulted| *defaulted) {
            return false;
        }
        let mut arguments = Vec::new();
        collect_abs(key, &mut arguments);
        arguments.into_iter().any(|argument| {
            let on_default = free_vars(argument).iter().any(|name| {
                self.vars
                    .iter()
                    .position(|listed| listed == name)
                    .is_some_and(|index| self.defaulted[index])
            });
            on_default && self.keeps_one_sign(argument)
        })
    }

    /// Whether the argument has one sign at each sample point, in both constant
    /// environments. A value of 0 or one that is not finite does not refuse.
    fn keeps_one_sign(&self, argument: &Ast) -> bool {
        constant_values(self.up_to_constant)
            .iter()
            .all(|&constant| {
                let mut seen_negative: Option<bool> = None;
                for env in self.points_with_constants(constant) {
                    let Some(value) = eval(argument, &env) else {
                        return false;
                    };
                    if value == 0.0 {
                        return false;
                    }
                    let negative = value < 0.0;
                    if seen_negative.is_some_and(|seen| seen != negative) {
                        return false;
                    }
                    seen_negative = Some(negative);
                }
                true
            })
    }

    /// Whether each sample value of an author domain sits on a multiple of
    /// 1/4 (D46): a multiple of `pi x` then has one value at each point. The
    /// defaults never do, so only a written domain triggers the refusal.
    fn quarter_points(&self) -> bool {
        self.defaulted.iter().all(|defaulted| !defaulted)
            && self.sample_points().iter().all(|env| {
                self.vars
                    .iter()
                    .all(|name| env[name] * 4.0 == (env[name] * 4.0).trunc())
            })
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
/// Returns [`Undecidable`] for a key outside the rules: not one expression, a
/// name outside its variables, `log` or the e notation (D28), a domain that
/// puts each sample point on a quarter (D46), fewer than [`MIN_FINITE_POINTS`]
/// finite points, a constant that is not additive (D44), or an `abs` argument
/// that keeps one sign on the default domain (D45).
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
    if spec.quarter_points() {
        return Err(Undecidable::new(QUARTER_POINTS));
    }
    let spec = spec.for_key(formula);
    if spec.finite_points(formula) < MIN_FINITE_POINTS {
        return Err(Undecidable::new(
            "the authored function has fewer than six finite sample points",
        ));
    }
    if spec.up_to_constant && !spec.constant_is_additive(formula) {
        return Err(Undecidable::new(NOT_ADDITIVE));
    }
    if spec.hides_a_sign(formula) {
        return Err(Undecidable::new(UNTESTED_ABS));
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
/// is finite is wrong; (4) without `up_to_constant`, each difference is within
/// `TOLERANCE * max(1, |key|)` (D43); (5) with `up_to_constant`, the change of
/// the two formulas from the first point agrees (D42), with each constant name
/// at 0 and again at 1 (D34).
#[must_use]
pub fn grade(expected: &Ast, learner: &Ast, spec: &FunctionSpec) -> Verdict {
    let (expected, learner) = (body(expected), body(learner));
    let spec = spec.for_key(expected);
    let same_form =
        matches!((canon(expected), canon(learner)), (Ok(key), Ok(answer)) if key == answer);
    Verdict {
        correct: same_form || (spec.permits_each_name(learner) && agrees(expected, learner, &spec)),
        notation: false,
    }
}

/// Whether the two formulas agree. With `up_to_constant` the rule must hold with
/// each constant name at 0 and again at 1 (D34): with 0 only, `C*x` gets "correct".
fn agrees(expected: &Ast, learner: &Ast, spec: &FunctionSpec) -> bool {
    constant_values(spec.up_to_constant)
        .iter()
        .all(|constant| agrees_at(expected, learner, spec, *constant))
}

/// The constant values that a grade reads: with `up_to_constant`, each constant
/// name is graded at 0 and again at 1 (D34).
fn constant_values(up_to_constant: bool) -> &'static [f64] {
    if up_to_constant { &[0.0, 1.0] } else { &[0.0] }
}

fn agrees_at(expected: &Ast, learner: &Ast, spec: &FunctionSpec, constant: f64) -> bool {
    let mut pairs = Vec::new();
    for env in spec.points_with_constants(constant) {
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
        variation(&pairs)
    } else {
        pairs
            .iter()
            .all(|(key, answer)| (key - answer).abs() <= TOLERANCE * key.abs().max(1.0))
    }
}

/// Whether the change of the two formulas from the first pair agrees at each
/// point (D42): `|(k_j - k_0) - (l_j - l_0)| <= TOLERANCE * max(1, |k_j|,
/// |k_0|)`. The scale reads key values only. The old spread rule made one
/// large learner constant correct for every key.
fn variation(pairs: &[(f64, f64)]) -> bool {
    let (first_key, first_answer) = pairs[0];
    pairs.iter().all(|(key, answer)| {
        ((key - first_key) - (answer - first_answer)).abs()
            <= TOLERANCE * key.abs().max(first_key.abs()).max(1.0)
    })
}

/// Collect the argument of each `abs` node, in reading order.
fn collect_abs<'a>(tree: &'a Ast, arguments: &mut Vec<&'a Ast>) {
    match tree {
        Ast::Func(name, items) => {
            if name == "abs" && items.len() == 1 {
                arguments.push(&items[0]);
            }
            for item in items {
                collect_abs(item, arguments);
            }
        }
        Ast::Sqrt(inner) | Ast::Neg(inner) => collect_abs(inner, arguments),
        Ast::Pow(base, _) => collect_abs(base, arguments),
        Ast::RationalPow { base, .. } => collect_abs(base, arguments),
        Ast::Div(left, right) => {
            collect_abs(left, arguments);
            collect_abs(right, arguments);
        }
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items) => {
            items.iter().for_each(|item| collect_abs(item, arguments));
        }
        Ast::Interval { lo, hi, .. } | Ast::Chain { lo, hi, .. } => {
            collect_abs(lo, arguments);
            collect_abs(hi, arguments);
        }
        Ast::Ineq { bound, .. }
        | Ast::Quantity { value: bound, .. }
        | Ast::Assign { value: bound, .. } => collect_abs(bound, arguments),
        Ast::Integer(_)
        | Ast::Decimal { .. }
        | Ast::Fraction { .. }
        | Ast::Mixed { .. }
        | Ast::Var(_)
        | Ast::Const(_) => {}
    }
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
