//! Setup-preserving equality comparison for unsolved translation exercises.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use super::{Canon, Undecidable, canonical_form, relation};
use crate::answer::{ast::Ast, parse};

pub(super) fn read(text: &str) -> Result<Canon, Undecidable> {
    let (left_text, right_text) = relation::setup_sides(text)?;
    let left = parse(&left_text)?;
    let right = parse(&right_text)?;
    let mut variables = BTreeSet::new();
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
        Ast::Mul(items) => commutative("multiply", items, variables)?,
        Ast::Div(left, right) => node(
            "divide",
            vec![shape(left, variables)?, shape(right, variables)?],
        ),
        _ => return Err(refusal()),
    };
    Ok(shaped)
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
