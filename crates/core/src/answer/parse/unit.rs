//! The value-with-unit production (D-F3): a number and one unit token.

use super::Parser;
use crate::answer::ast::Ast;
use crate::answer::lexer::{Tok, Token};
use crate::answer::unit::lookup;

impl Parser<'_> {
    /// Read the rest of the answer as a number with a unit, if it is one.
    ///
    /// The unit stands last, and a currency sign stands first or last. The
    /// head in front of the unit is one number expression: literals, the
    /// constants, roots, and arithmetic, with no variable and no function. A
    /// head outside that shape leaves the answer to the ordinary productions,
    /// so `2x + h` keeps its variable `h`, `x m` is the product `x*m`, and
    /// `cos 70°` is a unit inside an expression, which the grammar refuses.
    ///
    /// A one-letter unit needs a space in front of it: `5 m` is five meters and
    /// `5m` is the product `5*m`, which is the algebra spelling of the corpus.
    /// A longer unit, a compound unit, and a glyph read glued or spaced:
    /// `5cm`, `60km/h`, `90°`, and `5€` are quantities.
    pub(super) fn read_quantity(&mut self) -> Option<Ast> {
        let rest = self.tokens.get(self.at..)?;
        let (unit, head) = unit_split(rest)?;
        if head.is_empty() {
            return None;
        }
        let value = self.parse_body(head, "a quantity with no value").ok()?;
        if !is_number_expression(&value) {
            return None;
        }
        self.at = self.tokens.len();
        Some(Ast::Quantity {
            value: Box::new(value),
            unit,
        })
    }
}

/// Split the tokens into the unit spelling and the head in front of it.
fn unit_split(tokens: &[Token]) -> Option<(&'static str, &[Token])> {
    if let Some(found) = leading_currency(tokens) {
        return Some(found);
    }
    if let Some(found) = compound_unit(tokens) {
        return Some(found);
    }
    let (last, head) = tokens.split_last()?;
    let unit = match &last.kind {
        Tok::Unit(glyph) => lookup(glyph)?,
        Tok::Ident(name) if spaced_or_long(name, last) => lookup(name)?,
        _ => return None,
    };
    Some((unit.spelling, head))
}

/// Read a currency sign in front of the number: `$5` and `€5`.
fn leading_currency(tokens: &[Token]) -> Option<(&'static str, &[Token])> {
    let (first, head) = tokens.split_first()?;
    let Tok::Unit(sign) = &first.kind else {
        return None;
    };
    if sign != "$" && sign != "€" {
        return None;
    }
    Some((lookup(sign)?.spelling, head))
}

/// Read a compound unit of three tokens at the end: `km/h`, `m/s`, `cm^2`, `m^3`.
fn compound_unit(tokens: &[Token]) -> Option<(&'static str, &[Token])> {
    let split = tokens.len().checked_sub(3)?;
    let (head, tail) = tokens.split_at(split);
    let [first, middle, last] = tail else {
        return None;
    };
    let Tok::Ident(name) = &first.kind else {
        return None;
    };
    let spelling = match (&middle.kind, &last.kind) {
        (Tok::Slash, Tok::Ident(divisor)) => format!("{name}/{divisor}"),
        (Tok::Pow, Tok::Num(power)) => format!("{name}^{power}"),
        _ => return None,
    };
    if !spaced_or_long(name, first) {
        return None;
    }
    Some((lookup(&spelling)?.spelling, head))
}

/// Whether a unit spelled with letters takes its unit reading here.
///
/// A one-letter spelling needs a space in front of it; a longer one does not.
fn spaced_or_long(name: &str, token: &Token) -> bool {
    name.chars().count() > 1 || token.space_before
}

/// Whether a tree is a number expression: literals, constants, roots, and
/// arithmetic, with no variable and no function.
fn is_number_expression(value: &Ast) -> bool {
    match value {
        Ast::Integer(_)
        | Ast::Decimal { .. }
        | Ast::Fraction { .. }
        | Ast::Mixed { .. }
        | Ast::Const(_) => true,
        Ast::Neg(inner) | Ast::Sqrt(inner) | Ast::Pow(inner, _) => is_number_expression(inner),
        Ast::RationalPow { base, .. } => is_number_expression(base),
        Ast::Add(items) | Ast::Mul(items) => items.iter().all(is_number_expression),
        Ast::Div(left, right) => is_number_expression(left) && is_number_expression(right),
        _ => false,
    }
}
