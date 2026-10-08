//! The syntactic forms of [`super::form`] that read a tree closely: a rational
//! expression in lowest terms, the vertex form of a parabola, a sum of place
//! values, a repeated product, a fraction with the key's denominator, and the
//! three forms of the equation of a line.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::form::NumericForm;
use crate::answer::{Ast, Atom, Canon, Poly, canon, normalize, parse};

fn strip_neg(node: &Ast) -> &Ast {
    match node {
        Ast::Neg(inner) => strip_neg(inner),
        other => other,
    }
}

/// Whether a division sits anywhere inside `node`.
fn holds_div(node: &Ast) -> bool {
    match node {
        Ast::Div(..) | Ast::Fraction { .. } | Ast::Mixed { .. } => true,
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => holds_div(inner),
        Ast::RationalPow { base, .. } => holds_div(base),
        Ast::Add(items) | Ast::Mul(items) => items.iter().any(holds_div),
        Ast::Func(_, args) => args.iter().any(holds_div),
        _ => false,
    }
}

/// Whether a decimal literal sits anywhere inside `node`.
fn holds_decimal(node: &Ast) -> bool {
    match node {
        Ast::Decimal { .. } => true,
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => holds_decimal(inner),
        Ast::RationalPow { base, .. } => holds_decimal(base),
        Ast::Add(items) | Ast::Mul(items) => items.iter().any(holds_decimal),
        Ast::Div(top, bottom) => holds_decimal(top) || holds_decimal(bottom),
        Ast::Func(_, args) => args.iter().any(holds_decimal),
        _ => false,
    }
}

/// A rational expression in lowest terms, or an expression with no fraction.
/// A decimal is not a fraction in lowest terms, so a decimal literal refuses.
pub(super) fn simplified(tree: &Ast) -> bool {
    if holds_decimal(tree) {
        return false;
    }
    match strip_neg(tree) {
        Ast::Mul(items)
            if items
                .iter()
                .any(|item| matches!(item, Ast::Fraction { .. })) =>
        {
            // A fraction coefficient times a variable part: `(3/5)x`.
            let fractions = items
                .iter()
                .filter(|item| matches!(item, Ast::Fraction { .. }))
                .count();
            fractions == 1
                && items.iter().all(|item| match item {
                    Ast::Fraction { .. } => simplified(item),
                    other => !holds_div(other),
                })
        }
        Ast::Fraction {
            numerator,
            denominator,
        } => denominator.is_positive() && numerator.gcd(denominator).is_one(),
        Ast::Div(numerator, denominator) => {
            !holds_div(numerator) && !holds_div(denominator) && lowest_terms(numerator, denominator)
        }
        other => !holds_div(other),
    }
}

/// The polynomial of a canonical value, if it is one.
fn as_poly(value: Canon) -> Option<Poly> {
    match value {
        Canon::Rational(number) => {
            let mut poly = Poly::new();
            if !number.is_zero() {
                poly.insert(BTreeMap::new(), number);
            }
            Some(poly)
        }
        Canon::Poly(poly) => Some(poly),
        _ => None,
    }
}

fn lowest_terms(numerator: &Ast, denominator: &Ast) -> bool {
    let (Some(top), Some(bottom)) = (
        canon(numerator).ok().and_then(as_poly),
        canon(denominator).ok().and_then(as_poly),
    ) else {
        return false;
    };
    if top.is_empty() || bottom.is_empty() {
        return false;
    }
    let content = |poly: &Poly| -> Option<BigInt> {
        let mut gcd = BigInt::zero();
        for coefficient in poly.values() {
            if !coefficient.is_integer() {
                return None;
            }
            gcd = gcd.gcd(coefficient.numer());
        }
        Some(gcd)
    };
    let (Some(top_content), Some(bottom_content)) = (content(&top), content(&bottom)) else {
        return false;
    };
    if !top_content.gcd(&bottom_content).is_one() {
        return false;
    }
    // A variable that divides every term of the numerator and of the denominator.
    let floor = |poly: &Poly| -> BTreeMap<Atom, i64> {
        let mut floor: Option<BTreeMap<Atom, i64>> = None;
        for monomial in poly.keys() {
            floor = Some(match floor {
                None => monomial.clone(),
                Some(previous) => previous
                    .into_iter()
                    .filter_map(|(atom, exponent)| {
                        let other = monomial.get(&atom).copied().unwrap_or(0);
                        let least = exponent.min(other);
                        (least > 0).then_some((atom, least))
                    })
                    .collect(),
            });
        }
        floor.unwrap_or_default()
    };
    let (top_floor, bottom_floor) = (floor(&top), floor(&bottom));
    if top_floor.iter().any(|(atom, exponent)| {
        *exponent > 0 && bottom_floor.get(atom).is_some_and(|other| *other > 0)
    }) {
        return false;
    }
    univariate_coprime(&top, &bottom)
}

/// The coefficients by exponent, when the polynomial has one variable only.
fn dense(poly: &Poly, name: &str) -> Option<Vec<BigRational>> {
    let mut out: Vec<BigRational> = Vec::new();
    for (monomial, coefficient) in poly {
        let exponent = match monomial.iter().next() {
            None => 0,
            Some((Atom::Var(var), exponent)) if monomial.len() == 1 && var == name => {
                usize::try_from(*exponent).ok()?
            }
            _ => return None,
        };
        if exponent > 64 {
            return None;
        }
        if out.len() <= exponent {
            out.resize(exponent + 1, BigRational::zero());
        }
        out[exponent] = coefficient.clone();
    }
    Some(out)
}

fn variable_of(poly: &Poly) -> Option<String> {
    poly.keys()
        .flat_map(|monomial| monomial.keys())
        .find_map(|atom| match atom {
            Atom::Var(name) => Some(name.clone()),
            _ => None,
        })
}

/// Whether two polynomials of one variable share no factor of positive degree.
fn univariate_coprime(top: &Poly, bottom: &Poly) -> bool {
    let Some(name) = variable_of(top).or_else(|| variable_of(bottom)) else {
        return true;
    };
    let (Some(mut a), Some(mut b)) = (dense(top, &name), dense(bottom, &name)) else {
        // Two variables: only the monomial and the number checks apply.
        return true;
    };
    trim(&mut a);
    trim(&mut b);
    while !b.is_empty() {
        let rest = remainder(&a, &b);
        a = b;
        b = rest;
    }
    a.len() <= 1
}

fn trim(poly: &mut Vec<BigRational>) {
    while poly.last().is_some_and(Zero::is_zero) {
        poly.pop();
    }
}

fn remainder(a: &[BigRational], b: &[BigRational]) -> Vec<BigRational> {
    let mut rest = a.to_vec();
    let Some(lead) = b.last() else {
        return rest;
    };
    while rest.len() >= b.len() {
        let Some(top) = rest.last().cloned() else {
            break;
        };
        let factor = top / lead;
        let shift = rest.len() - b.len();
        for (at, coefficient) in b.iter().enumerate() {
            rest[shift + at] -= &factor * coefficient;
        }
        rest.pop();
        trim(&mut rest);
    }
    rest
}

fn number_literal(node: &Ast) -> bool {
    match strip_neg(node) {
        Ast::Integer(_) | Ast::Decimal { .. } => true,
        Ast::Fraction {
            numerator,
            denominator,
        } => denominator.is_positive() && numerator.gcd(denominator).is_one(),
        _ => false,
    }
}

/// `a(x - h)^2 + k`: the label of the learner (`y =`) is not part of the form.
pub(super) fn vertex(tree: &Ast) -> bool {
    let tree = match tree {
        Ast::Assign { value, .. } => value.as_ref(),
        other => other,
    };
    let terms: Vec<&Ast> = match tree {
        Ast::Add(terms) => terms.iter().collect(),
        other => vec![other],
    };
    let squares = terms.iter().filter(|term| squared_term(term)).count();
    let constants = terms.iter().filter(|term| number_literal(term)).count();
    squares == 1 && constants <= 1 && squares + constants == terms.len()
}

fn squared_term(node: &Ast) -> bool {
    match strip_neg(node) {
        Ast::Pow(base, 2) => binomial_base(base),
        Ast::Mul(factors) => {
            let squares: Vec<&Ast> = factors.iter().filter(|f| !number_literal(f)).collect();
            matches!(squares.as_slice(), [one] if squared_term(one))
        }
        Ast::Div(top, bottom) => number_literal(bottom) && squared_term(top),
        _ => false,
    }
}

fn binomial_base(base: &Ast) -> bool {
    match base {
        Ast::Var(_) => true,
        Ast::Add(items) => {
            let vars = items
                .iter()
                .filter(|item| matches!(strip_neg(item), Ast::Var(_)))
                .count();
            let numbers = items.iter().filter(|item| number_literal(item)).count();
            items.len() == 2 && vars == 1 && numbers == 1
        }
        _ => false,
    }
}

/// A sum of at least two place-value terms, each place once.
pub(super) fn place_value_sum(tree: &Ast) -> bool {
    let Ast::Add(terms) = tree else {
        return false;
    };
    let mut places = Vec::new();
    for term in terms {
        let Some(place) = place_of(term) else {
            return false;
        };
        if places.contains(&place) {
            return false;
        }
        places.push(place);
    }
    true
}

/// The power of ten of a term `d * 10^k`, or `None` when it is no such term.
fn place_of(node: &Ast) -> Option<i64> {
    match node {
        Ast::Integer(value) => digit_place(value, 0),
        Ast::Decimal { mantissa, scale } => digit_place(mantissa, -i64::from(*scale)),
        Ast::Mul(factors) => {
            let [digit, power] = factors.as_slice() else {
                return None;
            };
            let Ast::Integer(digit) = digit else {
                return None;
            };
            if !(BigInt::one()..=BigInt::from(9)).contains(digit) {
                return None;
            }
            ten_power(power)
        }
        other => ten_power(other),
    }
}

fn digit_place(value: &BigInt, base: i64) -> Option<i64> {
    let text = value.to_string();
    let mut chars = text.chars();
    let first = chars.next()?;
    if first == '0' || value.is_negative() || !chars.all(|c| c == '0') {
        return None;
    }
    Some(base + i64::try_from(text.len() - 1).ok()?)
}

fn ten_power(node: &Ast) -> Option<i64> {
    match node {
        Ast::Integer(value) => {
            let text = value.to_string();
            (text.starts_with('1') && text[1..].chars().all(|c| c == '0') && text.len() > 1)
                .then(|| i64::try_from(text.len() - 1).ok())
                .flatten()
        }
        Ast::Pow(base, exponent) if **base == Ast::Integer(10.into()) => Some(*exponent),
        _ => None,
    }
}

/// One factor written again and again: `5*5*5`, `x*x*x`. When the key is a
/// product, the factor list must be the factor list of the key, in any order:
/// `4*4` is not `2*2*2*2`.
pub(super) fn repeated(tree: &Ast, expected: &str) -> bool {
    let Ast::Mul(factors) = tree else {
        return false;
    };
    if let Ok(Ast::Mul(key_factors)) = parse(&normalize(expected).source) {
        let mut rest: Vec<&Ast> = key_factors.iter().collect();
        return factors.len() == rest.len()
            && factors.iter().all(|factor| {
                rest.iter()
                    .position(|other| *other == factor)
                    .map(|at| rest.swap_remove(at))
                    .is_some()
            });
    }
    let atomic = |node: &Ast| {
        matches!(
            strip_neg(node),
            Ast::Integer(_) | Ast::Decimal { .. } | Ast::Var(_)
        )
    };
    factors.len() >= 2 && atomic(&factors[0]) && factors.iter().all(|factor| *factor == factors[0])
}

/// A literal fraction with the denominator of the key.
pub(super) fn with_key_denominator(tree: &Ast, expected: &str) -> bool {
    let Ok(key) = parse(&normalize(expected).source) else {
        return false;
    };
    match (strip_neg(tree), strip_neg(&key)) {
        (Ast::Fraction { denominator: a, .. }, Ast::Fraction { denominator: b, .. }) => a == b,
        _ => false,
    }
}

/// Whether the text is `y` or a function label such as `f(x)`.
fn is_line_label(text: &str) -> bool {
    let label: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut chars = label.chars();
    label == "y"
        || matches!(
            (chars.next(), chars.next(), chars.next(), chars.next(), chars.next()),
            (Some(name), Some('('), Some('x'), Some(')'), None) if name.is_alphabetic()
        )
}

/// The text of an equation of a line, with a leading label `f(x) =` written as
/// `y =`: both name the same line.
pub(super) fn function_label_as_y(text: &str) -> String {
    match text.split_once('=') {
        Some((left, right)) if left.trim() != "y" && is_line_label(left) => {
            format!("y = {right}")
        }
        _ => text.to_owned(),
    }
}

/// The form of the text of an equation of a line.
pub(super) fn line(form: NumericForm, text: &str) -> bool {
    let source = normalize(text).source;
    let Some((left, right)) = source.split_once('=') else {
        return false;
    };
    if right.contains('=') {
        return false;
    }
    let side = |text: &str| parse(&normalize(text).source).ok();
    let named = is_line_label(left);
    let (Some(left), Some(right)) = (side(left), side(right)) else {
        return false;
    };
    match form {
        NumericForm::SlopeInterceptForm => named && slope_intercept_side(&right),
        NumericForm::StandardFormLine => standard_form(&left, &right),
        NumericForm::PointSlopeForm => point_slope(&left, &right),
        _ => false,
    }
}

/// `mx` or `mx + b` with the slope term first.
fn slope_intercept_side(right: &Ast) -> bool {
    let terms: Vec<&Ast> = match right {
        Ast::Add(terms) => terms.iter().collect(),
        other => vec![other],
    };
    match terms.as_slice() {
        [only] => number_literal(only) || slope_term(only),
        [first, second] => {
            (slope_term(first) && number_literal(second))
                || (number_literal(first) && slope_term(second))
        }
        _ => false,
    }
}

/// A term `x`, `3x`, `-x`, `x/2`, `(3/4)x` with the variable `x`. Each
/// coefficient is simplified: `4x/2` and `6/2` are not.
fn slope_term(node: &Ast) -> bool {
    match strip_neg(node) {
        Ast::Var(name) => name == "x",
        Ast::Mul(factors) => {
            matches!(factors.as_slice(), [number, var] if number_literal(number) && matches!(var, Ast::Var(name) if name == "x"))
        }
        Ast::Div(top, bottom) => {
            let Ast::Integer(divisor) = strip_neg(bottom) else {
                return false;
            };
            if divisor.is_one() || divisor.is_zero() {
                return false;
            }
            match strip_neg(top) {
                Ast::Var(name) => name == "x",
                Ast::Mul(factors) => matches!(
                    factors.as_slice(),
                    [Ast::Integer(coefficient), Ast::Var(name)]
                        if name == "x" && coefficient.gcd(divisor).is_one()
                ),
                _ => false,
            }
        }
        _ => false,
    }
}

/// The value of a whole-number literal: `3`, or `3.0` with only zeros after the point.
fn whole_literal(node: &Ast) -> Option<BigInt> {
    match node {
        Ast::Integer(value) => Some(value.clone()),
        Ast::Decimal { mantissa, scale } => {
            let power = BigInt::from(10).pow(*scale);
            (mantissa % &power).is_zero().then(|| mantissa / power)
        }
        _ => None,
    }
}

/// The whole coefficient and the name of a term `3x`, `x*3`, `-y`, `x`.
fn integer_term(node: &Ast) -> Option<(BigInt, String)> {
    let negative = matches!(node, Ast::Neg(_));
    let core = strip_neg(node);
    let (coefficient, name) = match core {
        Ast::Var(name) => (BigInt::one(), name.clone()),
        Ast::Mul(factors) => match factors.as_slice() {
            [number, Ast::Var(name)] | [Ast::Var(name), number] => {
                (whole_literal(number)?, name.clone())
            }
            _ => return None,
        },
        _ => return None,
    };
    Some((if negative { -coefficient } else { coefficient }, name))
}

/// `Ax + By = C` with whole `A`, `B`, `C` in lowest terms and `A > 0` (`B > 0`
/// when there is no `x` term). The terms may stand in either order.
fn standard_form(left: &Ast, right: &Ast) -> bool {
    let Some(c) = whole_literal(strip_neg(right)) else {
        return false;
    };
    let terms: Vec<&Ast> = match left {
        Ast::Add(terms) => terms.iter().collect(),
        other => vec![other],
    };
    let parsed: Option<Vec<(BigInt, String)>> =
        terms.iter().map(|term| integer_term(term)).collect();
    let Some(parsed) = parsed else {
        return false;
    };
    let mut names: Vec<&str> = parsed.iter().map(|(_, name)| name.as_str()).collect();
    names.sort_unstable();
    if !matches!(names.as_slice(), ["x"] | ["y"] | ["x", "y"]) {
        return false;
    }
    let leading = parsed
        .iter()
        .find(|(_, name)| name == "x")
        .or_else(|| parsed.first());
    let gcd = parsed
        .iter()
        .fold(c.abs(), |acc, (coefficient, _)| acc.gcd(&coefficient.abs()));
    leading.is_some_and(|(first, _)| first.is_positive()) && gcd.is_one()
}

fn point_slope(left: &Ast, right: &Ast) -> bool {
    point_slope_sides(left, right) || point_slope_sides(right, left)
}

/// `y - y1` on one side and `m(x - x1)` on the other.
fn point_slope_sides(left: &Ast, right: &Ast) -> bool {
    let shifted = |node: &Ast, name: &str| match node {
        Ast::Var(var) => var == name,
        Ast::Add(items) => {
            matches!(items.as_slice(), [Ast::Var(var), shift] if var == name && number_literal(shift))
        }
        _ => false,
    };
    let slope_side = match right {
        Ast::Mul(factors) => match factors.as_slice() {
            [slope, rest] => number_literal(slope) && shifted(rest, "x"),
            _ => false,
        },
        other => shifted(other, "x"),
    };
    shifted(left, "y") && slope_side
}
