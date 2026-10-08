//! Exact normalization for reviewed polynomial relations.

use std::collections::BTreeMap;

use num_traits::{Signed, Zero};

use super::{Canon, Undecidable, canonical_form};
use crate::answer::{Atom, Monomial, Outcome, Poly, Verdict, normalize};

#[derive(Clone, Copy)]
enum Relation {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Relation {
    fn label(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }

    fn reversed(self) -> Self {
        match self {
            Self::Eq => Self::Eq,
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
        }
    }
}

/// Read one polynomial relation and normalize it to `monic polynomial OP 0`.
pub(super) fn read(text: &str) -> Result<Canon, Undecidable> {
    let source = normalize(&hat_names(text)).source;
    let (left, relation, right) = split(&source)?;
    let mut polynomial = as_poly(canonical_form(left)?)?;
    for (monomial, coefficient) in as_poly(canonical_form(right)?)? {
        let entry = polynomial.entry(monomial).or_default();
        *entry -= coefficient;
    }
    polynomial.retain(|_, coefficient| !coefficient.is_zero());
    if matches!(relation, Relation::Eq) {
        clear_denominators(&mut polynomial);
    }
    let Some(first) = polynomial.values().next().cloned() else {
        return Err(refusal());
    };
    let relation = if first.is_negative() && !matches!(relation, Relation::Eq) {
        relation.reversed()
    } else {
        relation
    };
    let divisor = first;
    for coefficient in polynomial.values_mut() {
        *coefficient /= &divisor;
    }
    Ok(Canon::Tuple(vec![
        Canon::Label(relation.label().to_owned()),
        Canon::Poly(polynomial),
    ]))
}

/// The verdict of a learner text against the authored relation `expected`.
///
/// A text with no comparison is an expression and not a relation, so it is wrong.
/// A relation followed by `v = 0` for a variable the key does not hold (the trace
/// `x + y = 3, z = 0` of the key `x + y = 3`) names the same relation in its plane.
pub(super) fn grade_learner(learner: &str, expected: &Canon) -> Outcome {
    let decided = |correct| {
        Outcome::Decided(Verdict {
            correct,
            notation: false,
        })
    };
    let refusal = match read(learner) {
        Ok(value) => return decided(&value == expected),
        Err(reason) => reason,
    };
    let source = normalize(learner).source;
    if !source.contains(['<', '>', '=']) && canonical_form(&source).is_ok() {
        return decided(false);
    }
    let parts: Vec<&str> = source.split(',').map(str::trim).collect();
    if let [first, second] = parts.as_slice() {
        for (main, plane) in [(first, second), (second, first)] {
            if read(main).is_ok_and(|value| &value == expected) && is_free_plane(plane, expected) {
                return decided(true);
            }
        }
    }
    Outcome::Undecidable(refusal)
}

/// Whether `text` reads `v = 0` for one variable `v` that the relation does not hold.
fn is_free_plane(text: &str, expected: &Canon) -> bool {
    let Some((name, zero)) = text.split_once('=') else {
        return false;
    };
    let name = name.trim();
    let single = name.chars().count() == 1 && name.chars().all(|ch| ch.is_ascii_alphabetic());
    let Canon::Tuple(items) = expected else {
        return false;
    };
    let held = items.iter().any(|item| {
        matches!(item, Canon::Poly(poly) if poly.keys().any(|monomial| monomial.contains_key(&Atom::Var(name.to_owned()))))
    });
    single && !held && zero.trim() == "0"
}

/// The hat names of a regression line (`ŷ`, `yhat`, `y_hat`) written as one name
/// `y_hat`, which the reader takes as one variable.
fn hat_names(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        let next = chars.get(at + 1).copied();
        if ch == 'ŷ' {
            out.push_str("y_hat");
        } else if ch.is_ascii_alphabetic() && next == Some('\u{302}') {
            out.push_str(&format!("{ch}_hat"));
            at += 1;
        } else if ch.is_ascii_alphabetic()
            && !at
                .checked_sub(1)
                .is_some_and(|before| chars[before].is_alphanumeric())
            && chars[at + 1..].starts_with(&['h', 'a', 't'])
            && !chars
                .get(at + 4)
                .is_some_and(|after| after.is_alphanumeric())
        {
            out.push_str(&format!("{ch}_hat"));
            at += 3;
        } else {
            out.push(ch);
        }
        at += 1;
    }
    out
}

/// Multiply an equation by the lowest power of each atom that leaves no negative
/// exponent: `y - 1/x - 1 = 0` becomes `xy - 1 - x = 0`, so `y = 1/x + 1` and
/// `xy = 1 + x` read the same. An inequality keeps its text, because a factor
/// of unknown sign would flip it.
fn clear_denominators(polynomial: &mut Poly) {
    let mut shift: BTreeMap<crate::answer::Atom, i64> = BTreeMap::new();
    for monomial in polynomial.keys() {
        for (atom, power) in monomial {
            if *power < 0 {
                let entry = shift.entry(atom.clone()).or_insert(0);
                *entry = (*entry).max(-power);
            }
        }
    }
    if shift.is_empty() {
        return;
    }
    *polynomial = std::mem::take(polynomial)
        .into_iter()
        .map(|(mut monomial, coefficient)| {
            for (atom, power) in &shift {
                *monomial.entry(atom.clone()).or_insert(0) += power;
            }
            monomial.retain(|_, power| *power != 0);
            (monomial, coefficient)
        })
        .collect();
}

/// Whether the highest-degree term of the relation, moved to the side the
/// learner wrote it on, has the coefficient 1: `x^2 + 2x - 15 = 0` and
/// `0 = x^2 + 2x - 15` are monic, `2x^2 + 4x - 30 = 0` and `-x^2 - 2x + 15 = 0`
/// are not.
pub(super) fn is_monic(text: &str) -> bool {
    let source = normalize(text).source;
    let Ok((left, _, right)) = split(&source) else {
        return false;
    };
    let (Ok(left), Ok(right)) = (
        canonical_form(left).and_then(as_poly),
        canonical_form(right).and_then(as_poly),
    ) else {
        return false;
    };
    let degree = |monomial: &Monomial| monomial.values().sum::<i64>();
    let lead = |poly: &Poly| {
        poly.iter()
            .max_by_key(|(monomial, _)| (degree(monomial), (*monomial).clone()))
            .map(|(monomial, coefficient)| (degree(monomial), coefficient.clone()))
    };
    let mut difference = left.clone();
    for (monomial, coefficient) in &right {
        *difference.entry(monomial.clone()).or_default() -= coefficient;
    }
    difference.retain(|_, coefficient| !coefficient.is_zero());
    let one = num_rational::BigRational::from_integer(1.into());
    match (lead(&difference), left.is_empty(), right.is_empty()) {
        (Some((_, coefficient)), _, true) => coefficient == one,
        (Some((_, coefficient)), true, _) => coefficient == -one,
        (Some((_, coefficient)), false, false) => coefficient == one,
        _ => false,
    }
}

pub(super) fn setup_sides(text: &str) -> Result<(String, String), Undecidable> {
    let source = normalize(text).source;
    let (left, _, right) = split(&source)?;
    Ok((left.to_owned(), right.to_owned()))
}

fn as_poly(value: Canon) -> Result<Poly, Undecidable> {
    match value {
        Canon::Poly(polynomial) => Ok(polynomial),
        Canon::Rational(number) => {
            let mut polynomial = BTreeMap::new();
            if !number.is_zero() {
                polynomial.insert(Monomial::new(), number);
            }
            Ok(polynomial)
        }
        // A constant with a root (`2*sqrt(2)`) is a polynomial of degree 0 whose
        // coefficients carry the root as an atom.
        Canon::Radical(parts) => {
            let mut polynomial = BTreeMap::new();
            for (basis, coefficient) in parts {
                let mut monomial = Monomial::new();
                if basis.radicand != 1.into() {
                    monomial.insert(Atom::Sqrt(basis.radicand), 1);
                }
                if basis.pi != 0 {
                    monomial.insert(Atom::Pi, basis.pi);
                }
                if basis.e != 0 {
                    monomial.insert(Atom::E, basis.e);
                }
                polynomial.insert(monomial, coefficient);
            }
            Ok(polynomial)
        }
        // A lone `abs(x)` is one atom with coefficient 1; other functions are no polynomial.
        Canon::Func(name, arguments) if name == "abs" => {
            let mut monomial = Monomial::new();
            monomial.insert(crate::answer::Atom::Call(name, arguments), 1);
            let mut polynomial = BTreeMap::new();
            polynomial.insert(monomial, num_rational::BigRational::from_integer(1.into()));
            Ok(polynomial)
        }
        _ => Err(refusal()),
    }
}

fn split(source: &str) -> Result<(&str, Relation, &str), Undecidable> {
    let mut depth = 0_i32;
    let bytes = source.as_bytes();
    let mut found = None;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'<' | b'>' | b'=' if depth == 0 => {
                let width = usize::from(bytes.get(at + 1) == Some(&b'=')) + 1;
                if found.is_some() {
                    return Err(refusal());
                }
                let relation = match &source[at..at + width] {
                    "=" => Relation::Eq,
                    "<" => Relation::Lt,
                    "<=" => Relation::Le,
                    ">" => Relation::Gt,
                    ">=" => Relation::Ge,
                    _ => return Err(refusal()),
                };
                found = Some((at, width, relation));
                at += width;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    let (at, width, relation) = found.ok_or_else(refusal)?;
    let left = source[..at].trim();
    let right = source[at + width..].trim();
    if left.is_empty() || right.is_empty() || depth != 0 {
        return Err(refusal());
    }
    Ok((left, relation, right))
}

fn refusal() -> Undecidable {
    Undecidable::new("a polynomial relation requires two polynomial expressions and one comparison")
}
