//! Setup-preserving equality comparison for unsolved translation exercises.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use super::{Canon, Undecidable, canonical_form, relation};
use crate::answer::{ast::Ast, parse};

pub(super) fn read(text: &str) -> Result<Canon, Undecidable> {
    let (mut left_text, mut right_text) = relation::setup_sides(text)?;
    let mut left = parse(&left_text)?;
    let mut right = parse(&right_text)?;
    let mut variables = BTreeSet::new();
    // `58 >= 8x + 10` is `8x + 10 <= 58` written the other way round: the
    // symbolic side is the left side of the shape.
    let mut left_variables = BTreeSet::new();
    shape(&left, &mut left_variables)?;
    // An equation keeps its sides: `26 = 3x + 5` is another setup than `3x + 5 = 26`.
    if left_variables.is_empty() && text.contains(['<', '>']) {
        std::mem::swap(&mut left_text, &mut right_text);
        std::mem::swap(&mut left, &mut right);
    }
    let left_shape = shape(&left, &mut variables)?;
    if variables.len() != 1 {
        return Err(refusal());
    }
    let mut right_variables = BTreeSet::new();
    let right_shape = shape(&right, &mut right_variables)?;
    if !right_variables.is_empty() || !matches!(canonical_form(&right_text)?, Canon::Rational(_)) {
        return Err(refusal());
    }
    Ok(Canon::Tuple(vec![
        relation::read(text)?,
        left_shape,
        right_shape,
    ]))
}

fn shape(ast: &Ast, variables: &mut BTreeSet<String>) -> Result<Canon, Undecidable> {
    let shaped = match ast {
        Ast::Integer(value) => number(BigRational::from_integer(value.clone())),
        Ast::Decimal { mantissa, scale } => number(BigRational::new(
            mantissa.clone(),
            BigInt::from(10_u8).pow(*scale),
        )),
        Ast::Fraction {
            numerator,
            denominator,
        } => number(BigRational::new(numerator.clone(), denominator.clone())),
        Ast::Mixed {
            whole,
            numerator,
            denominator,
        } => number(
            BigRational::from_integer(whole.clone())
                + BigRational::new(numerator.clone(), denominator.clone()),
        ),
        Ast::Var(name) => {
            variables.insert(name.to_ascii_lowercase());
            Canon::Label("variable".to_owned())
        }
        Ast::Neg(inner) => node("negate", vec![shape(inner, variables)?]),
        Ast::Add(items) => commutative("add", items, variables)?,
        Ast::Mul(items) => scaled_or_commutative(items, variables)?,
        Ast::Div(left, right) => {
            let (top, bottom) = (shape(left, variables)?, shape(right, variables)?);
            // A symbolic term over a number is a scaled term: `m/2` is `0.5m`.
            if is_number(&bottom) && !is_number(&top) {
                node("scale", vec![top])
            } else {
                node("divide", vec![top, bottom])
            }
        }
        Ast::Func(name, arguments) if name == "abs" && arguments.len() == 1 => {
            node("abs", vec![shape(&arguments[0], variables)?])
        }
        _ => return Err(refusal()),
    };
    Ok(shaped)
}

/// Whether a shape is a plain number.
fn is_number(shaped: &Canon) -> bool {
    matches!(shaped, Canon::Tuple(items) if matches!(items.first(), Some(Canon::Label(label)) if label == "number"))
}

/// A product of one symbolic factor and numbers is a scaled term, whatever the
/// number is (`0.5m`, `(1/2)m`, `m/2`). Every other product keeps its factors.
fn scaled_or_commutative(
    items: &[Ast],
    variables: &mut BTreeSet<String>,
) -> Result<Canon, Undecidable> {
    let shapes = items
        .iter()
        .map(|item| shape(item, variables))
        .collect::<Result<Vec<_>, _>>()?;
    let symbolic: Vec<&Canon> = shapes.iter().filter(|shaped| !is_number(shaped)).collect();
    if symbolic.len() == 1 && symbolic.len() < shapes.len() {
        return Ok(node("scale", vec![symbolic[0].clone()]));
    }
    commutative("multiply", items, variables)
}

fn commutative(
    name: &str,
    items: &[Ast],
    variables: &mut BTreeSet<String>,
) -> Result<Canon, Undecidable> {
    let mut children = items
        .iter()
        .map(|item| shape(item, variables))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    Ok(node(name, children))
}

fn number(value: BigRational) -> Canon {
    Canon::Tuple(vec![
        Canon::Label("number".to_owned()),
        Canon::Rational(if value.is_zero() {
            BigRational::default()
        } else {
            value
        }),
    ])
}

fn node(name: &str, children: Vec<Canon>) -> Canon {
    Canon::Tuple(vec![Canon::Label(name.to_owned()), Canon::Tuple(children)])
}

fn refusal() -> Undecidable {
    Undecidable::new(
        "a relation setup requires one symbolic left side and one exact numeric right side",
    )
}
