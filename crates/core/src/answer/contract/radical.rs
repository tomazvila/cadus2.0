//! Exact numeric values written as a reduced rational times one simplified square root.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use crate::answer::lexer::{Tok, Token, lex};
use crate::answer::{Ast, Canon, Undecidable, canon, normalize, parse};

struct RadicalShape {
    negative: bool,
    numerator: Option<BigInt>,
    denominator: Option<BigInt>,
    radicand: Option<BigInt>,
    sign_seen: bool,
}

impl RadicalShape {
    fn new() -> Self {
        Self {
            negative: false,
            numerator: None,
            denominator: None,
            radicand: None,
            sign_seen: false,
        }
    }

    fn collect(&mut self, ast: &Ast) -> bool {
        match ast {
            Ast::Neg(inner) if !self.sign_seen => {
                self.sign_seen = true;
                self.negative = true;
                self.collect(inner)
            }
            Ast::Mul(factors) => factors.iter().all(|factor| self.collect(factor)),
            Ast::Div(numerator, denominator) => {
                self.collect(numerator) && self.set_denominator(denominator)
            }
            Ast::Integer(value) => self.set_numerator(value),
            Ast::Fraction {
                numerator,
                denominator,
            } => self.set_fraction(numerator, denominator),
            Ast::Sqrt(radicand) => self.set_radicand(radicand),
            _ => false,
        }
    }

    fn set_numerator(&mut self, value: &BigInt) -> bool {
        if value.is_zero() || self.numerator.is_some() {
            return false;
        }
        if value.is_negative() {
            if self.sign_seen {
                return false;
            }
            self.sign_seen = true;
            self.negative = true;
        }
        self.numerator = Some(value.abs());
        true
    }

    fn set_denominator(&mut self, ast: &Ast) -> bool {
        let Ast::Integer(value) = ast else {
            return false;
        };
        if value <= &BigInt::one() || self.denominator.is_some() {
            return false;
        }
        self.denominator = Some(value.clone());
        true
    }

    fn set_fraction(&mut self, numerator: &BigInt, denominator: &BigInt) -> bool {
        if numerator.is_zero()
            || denominator <= &BigInt::one()
            || !numerator.gcd(denominator).is_one()
            || self.numerator.is_some()
            || self.denominator.is_some()
        {
            return false;
        }
        if numerator.is_negative() {
            if self.sign_seen {
                return false;
            }
            self.sign_seen = true;
            self.negative = true;
        }
        self.numerator = Some(numerator.abs());
        self.denominator = Some(denominator.clone());
        true
    }

    fn set_radicand(&mut self, ast: &Ast) -> bool {
        let Ast::Integer(value) = ast else {
            return false;
        };
        if value <= &BigInt::one() || self.radicand.is_some() {
            return false;
        }
        self.radicand = Some(value.clone());
        true
    }

    fn coefficient(&self) -> Option<BigRational> {
        let radicand = self.radicand.as_ref()?;
        if radicand <= &BigInt::one() {
            return None;
        }
        let numerator = self.numerator.clone().unwrap_or_else(BigInt::one);
        let denominator = self.denominator.clone().unwrap_or_else(BigInt::one);
        if numerator.is_one() && self.numerator.is_some() && self.denominator.is_none() {
            return None;
        }
        if !numerator.gcd(&denominator).is_one() {
            return None;
        }
        let numerator = if self.negative { -numerator } else { numerator };
        Some(BigRational::new(numerator, denominator))
    }
}

pub(super) fn expected(text: &str) -> Result<Canon, Undecidable> {
    match read(text)? {
        Some(value) => Ok(value),
        None => Err(refused()),
    }
}

pub(super) fn equivalent(expected: &str, learner: &str) -> Result<bool, Undecidable> {
    let Some(expected) = read(expected)? else {
        return Err(refused());
    };
    let Some(learner) = read(learner)? else {
        return Ok(false);
    };
    Ok(expected == learner)
}

fn read(text: &str) -> Result<Option<Canon>, Undecidable> {
    let source = normalize(text).source;
    let tokens = lex(&source)?;
    if negative_denominator(&tokens) {
        return Ok(None);
    }
    let ast = parse(&source)?;
    if rational_literal(&ast) {
        let value = canon(&ast)?;
        return Ok(matches!(value, Canon::Rational(_)).then_some(value));
    }

    let mut shape = RadicalShape::new();
    if !shape.collect(&ast) {
        return Ok(None);
    }
    let Some(coefficient) = shape.coefficient() else {
        return Ok(None);
    };
    let Some(radicand) = shape.radicand else {
        return Ok(None);
    };
    let value = canon(&ast)?;
    let Canon::Radical(terms) = &value else {
        return Ok(None);
    };
    let Some((basis, actual_coefficient)) = terms.iter().next() else {
        return Ok(None);
    };
    if terms.len() != 1
        || basis.pi != 0
        || basis.e != 0
        || basis.radicand != radicand
        || actual_coefficient != &coefficient
    {
        return Ok(None);
    }
    Ok(Some(value))
}

fn negative_denominator(tokens: &[Token]) -> bool {
    for (at, token) in tokens.iter().enumerate() {
        match &token.kind {
            Tok::Slash if starts_negative(&tokens[at + 1..]) => return true,
            Tok::Frac {
                numerator,
                denominator,
            } if starts_negative(denominator)
                || negative_denominator(numerator)
                || negative_denominator(denominator) =>
            {
                return true;
            }
            Tok::Sqrt(body) if negative_denominator(body) => return true,
            _ => {}
        }
    }
    false
}

fn starts_negative(tokens: &[Token]) -> bool {
    let mut at = 0_usize;
    while matches!(
        tokens.get(at).map(|token| &token.kind),
        Some(Tok::LParen | Tok::Plus)
    ) {
        at += 1;
    }
    matches!(tokens.get(at).map(|token| &token.kind), Some(Tok::Minus))
}

fn rational_literal(ast: &Ast) -> bool {
    match ast {
        Ast::Integer(_) => true,
        Ast::Fraction {
            numerator,
            denominator,
        } => {
            denominator > &BigInt::one()
                && numerator.gcd(denominator).is_one()
                && !numerator.is_zero()
        }
        Ast::Neg(inner) => match inner.as_ref() {
            Ast::Integer(value) => value.is_positive(),
            Ast::Fraction {
                numerator,
                denominator,
            } => {
                numerator.is_positive()
                    && denominator > &BigInt::one()
                    && numerator.gcd(denominator).is_one()
            }
            _ => false,
        },
        _ => false,
    }
}

fn refused() -> Undecidable {
    Undecidable::new(
        "a required simplest radical needs a reduced rational times one squarefree integer root",
    )
}
