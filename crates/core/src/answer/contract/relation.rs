//! Exact normalization for reviewed polynomial relations.

use std::collections::BTreeMap;

use num_traits::{Signed, Zero};

use super::{Canon, Undecidable, canonical_form};
use crate::answer::{Monomial, Poly, normalize};

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
    let source = normalize(text).source;
    let (left, relation, right) = split(&source)?;
    let mut polynomial = as_poly(canonical_form(left)?)?;
    for (monomial, coefficient) in as_poly(canonical_form(right)?)? {
        let entry = polynomial.entry(monomial).or_default();
        *entry -= coefficient;
    }
    polynomial.retain(|_, coefficient| !coefficient.is_zero());
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
