//! The sum and product productions: terms, factors, and the mixed number.

use super::build::{
    collapse, digit_run_body, is_numeric_literal, is_times_letter, make_quotient,
    proper_fraction_part, signed_whole,
};
use super::{FractionPart, Parser};
use crate::answer::Undecidable;
use crate::answer::ast::Ast;
use crate::answer::lexer::Tok;

impl Parser<'_> {
    /// Parse a sum: `term (('+' | '-') term)*`.
    pub(super) fn parse_expr(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            let mut terms = vec![parser.parse_term()?];
            loop {
                if parser.eat(&Tok::Plus) {
                    terms.push(parser.parse_term()?);
                } else if parser.eat(&Tok::Minus) {
                    terms.push(Ast::Neg(Box::new(parser.parse_term()?)));
                } else {
                    break;
                }
            }
            Ok(collapse(terms, Ast::Add))
        })
    }

    /// Parse a product, including implicit multiplication and mixed numbers.
    fn parse_term(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            let mut factors = vec![parser.parse_unary()?];
            loop {
                if parser.eat(&Tok::Star) {
                    factors.push(parser.parse_unary()?);
                    continue;
                }
                if parser.eat(&Tok::Slash) {
                    let divisor = parser.parse_unary()?;
                    let dividend = collapse(std::mem::take(&mut factors), Ast::Mul);
                    factors.push(make_quotient(dividend, divisor)?);
                    continue;
                }
                if let Some(mixed) = parser.read_mixed_number(&factors)? {
                    factors = vec![mixed];
                    continue;
                }
                if parser.eat_times_letter(factors.last()) {
                    factors.push(parser.parse_unary()?);
                    continue;
                }
                if parser.starts_operand() {
                    if matches!(parser.peek(), Some(Tok::Num(_))) {
                        parser.check_implicit_number(factors.last())?;
                    }
                    factors.push(parser.parse_unary()?);
                    continue;
                }
                break;
            }
            Ok(collapse(factors, Ast::Mul))
        })
    }

    /// Refuse a number that follows an operand where it reads as a label, not a product.
    ///
    /// Three rules, and all of them come from how a learner writes:
    ///
    /// - A number after a number is never a product. `2 3` is a typing slip, and
    ///   `9 R2` is a quotient with a remainder (spec section 8.3), not `9*R*2`.
    ///   The rule reads through a leading sign, so `-2 3` is a slip too.
    /// - A number glued to a name is a label: `R2`, `H1`, `x2` name one thing. A
    ///   space makes it a product, which is how `6 y 10**3` reads.
    /// - A space-grouped number is one value on a full match of the whole string
    ///   and nowhere else (the V4 table). After a factor, the second group of
    ///   `x/1 000` is not the factor 0, so the answer is undecidable (review
    ///   round 2, finding #11).
    pub(super) fn check_implicit_number(&self, previous: Option<&Ast>) -> Result<(), Undecidable> {
        if previous.is_some_and(is_numeric_literal) {
            return Err(Undecidable::new("two numbers stand side by side"));
        }
        let Some(token) = self.tokens.get(self.at) else {
            return Err(Undecidable::new("the answer ends where a value belongs"));
        };
        if !token.space_before {
            return Err(Undecidable::new(
                "a number glued to a name reads as a label",
            ));
        }
        if self.continues_a_space_group(&token.kind) {
            return Err(Undecidable::new(
                "a space-grouped number stands after a factor",
            ));
        }
        Ok(())
    }

    /// Whether the spaced number at the cursor is one group of a grouped number.
    ///
    /// The V4 table deletes the separators of `1 000` on a full match of the
    /// whole answer and nowhere else. After a factor, the second group reaches
    /// the parser as its own number, so `x/1 000` reads as `x/1 * 0` and gives
    /// the value 0 that no learner wrote (review round 2, finding #11).
    ///
    /// Two shapes are a group and no factor:
    ///
    /// - three digits with a number token in front of them, which is the shape
    ///   of `1 000`, `2 500`, and `1 999`;
    /// - a run of more than one digit that starts with a zero, which no learner
    ///   writes as a factor.
    ///
    /// `x 100` holds no group, because no number stands in front of the run, so
    /// it keeps the product reading that 1.0 gives it.
    fn continues_a_space_group(&self, kind: &Tok) -> bool {
        let Tok::Num(text) = kind else {
            return false;
        };
        if !text.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if text.chars().count() > 1 && text.starts_with('0') {
            return true;
        }
        let before = self.at.checked_sub(1).and_then(|at| self.tokens.get(at));
        text.chars().count() == 3 && matches!(before.map(|token| &token.kind), Some(Tok::Num(_)))
    }

    /// Whether the cursor is on a token that can start a factor.
    ///
    /// `\sqrt{2}` and `√2` are factors of a product, so `5x\sqrt{2}` is
    /// `5*x*sqrt(2)` and `2\times\sqrt{3}` is `2*sqrt(3)`. Round 2 wrote a
    /// product sign into the source for the same reading, and that sign landed
    /// on the last letter of `\cdot` (review round 3, finding #8). A token needs
    /// no sign.
    pub(super) fn starts_operand(&self) -> bool {
        matches!(
            self.peek(),
            Some(
                Tok::Num(_)
                    | Tok::Ident(_)
                    | Tok::LParen
                    | Tok::Frac { .. }
                    | Tok::Sqrt(_)
                    | Tok::Root
            )
        )
    }

    /// Take a spaced `x` or `X` that stands between two numbers, which means times.
    ///
    /// 27 authored corpus answers of 5 topics write the times sign as `x`
    /// (`6 x 10^3`, `2 x 2 x 3`). A learner writes the same sign in upper case,
    /// so `6 X 10^3` is 6000 too (review round 1, the times-`x` ruling). Every
    /// other `x` and `X` is the variable, so the reading asks for a space on both
    /// sides and a number literal on both sides (review finding #18). `X` alone
    /// and `2X` therefore stay the variable.
    ///
    /// The literal on the left carries its sign, because 22 of the 27 authored
    /// times-`x` answers are scientific notation and a measurement is negative:
    /// `-2.5 x 10^-4` is -0.00025 (review round 2, finding #12).
    pub(super) fn eat_times_letter(&mut self, previous: Option<&Ast>) -> bool {
        if !previous.is_some_and(is_numeric_literal) {
            return false;
        }
        let Some(token) = self.tokens.get(self.at) else {
            return false;
        };
        if !token.space_before || !is_times_letter(&token.kind) {
            return false;
        }
        let Some(next) = self.tokens.get(self.at + 1) else {
            return false;
        };
        if !next.space_before || !matches!(next.kind, Tok::Num(_)) {
            return false;
        }
        self.bump();
        true
    }

    /// Read the mixed number that a number token in front of a fraction makes.
    ///
    /// This function is the one place that reads a mixed number. It takes all
    /// five spellings of the review round 2 ruling, because the fraction reaches
    /// it in one of two token shapes:
    ///
    /// - `Num Slash Num` after a space, which is `2 1/2`. The space is what tells
    ///   `3 1/2` from `31/2`, and the lexer joins two glued digit runs anyway.
    /// - one [`Tok::Frac`] token, which is `2½`, `2 ½`, `2\frac{1}{2}`, and
    ///   `2 \frac{1}{2}`. [`crate::answer::normalize`] writes every glyph and
    ///   every literal `\frac` in that one spelling, glued or spaced.
    ///
    /// The fractional part must be proper and plainly written: `0 < b < c` and no
    /// leading zero. The digit-run spelling adds one rule of its own: a
    /// three-digit numerator after a space is the thousands group of the V4
    /// table, so `1 000/3` and `1 200/300` stay undecidable and never become a
    /// value the checker invented (finding #7).
    ///
    /// A number token in front of a fraction is a mixed number or it is nothing.
    /// `2\frac{3}{2}` is neither the mixed number 7/2 nor the product 3, and
    /// `x 2½` carries no whole part at all; a checker that picks one of the two
    /// readings grades a wrong answer correct (C4). The `b/c` spelling keeps its
    /// round 1 refusal ("two numbers stand side by side") in the same shapes,
    /// which the caller raises.
    ///
    /// A token that is no number in front of the fraction makes an ordinary
    /// product, so `x½` is `x/2` and `(2)½` is 1.
    ///
    /// A `/` or a `^` in front of the fraction takes the number token into a
    /// quotient or a power, and a token inside a factor is no whole part. The
    /// `b/c` spelling refuses that shape here, the same as the four other
    /// spellings do, so `t/4 3/4`, `x/2 1/2`, and `x^2 1/2` are undecidable
    /// (review round 4, finding #1).
    pub(super) fn read_mixed_number(
        &mut self,
        factors: &[Ast],
    ) -> Result<Option<Ast>, Undecidable> {
        let Some(part) = self.read_fraction_part() else {
            return Ok(None);
        };
        // The whole part is a bare number literal, and the token in front of the
        // fraction carries it. A bracketed value takes no mixed part.
        let previous = self.at.checked_sub(1).and_then(|at| self.tokens.get(at));
        if !matches!(previous.map(|token| &token.kind), Some(Tok::Num(_))) {
            return Ok(None);
        }
        let whole = match factors {
            [only] => signed_whole(only),
            _ => None,
        };
        let Some((negative, whole)) = whole else {
            // The `b/c` spelling hands the answer back to the caller only where
            // the caller refuses it as well: `9/2 1/2` and `x 2 1/2` end on a
            // number literal, and `check_implicit_number` raises "two numbers
            // stand side by side" for both.
            //
            // A `/` or a `^` folds the number token into an `Ast::Div` or an
            // `Ast::Pow`, and neither node is a number literal, so the caller
            // took the product reading: `t/4 3/4` became 3t/16 and graded a
            // wrong answer correct (C4). The token is inside a factor, so it is
            // no whole part, and the answer is undecidable (review round 4,
            // finding #1).
            if part.digit_run && factors.last().is_some_and(is_numeric_literal) {
                return Ok(None);
            }
            return Err(Undecidable::new(
                "a fraction stands after a number that is no whole part",
            ));
        };
        let Some((numerator, denominator)) = proper_fraction_part(&part) else {
            if part.digit_run {
                return Ok(None);
            }
            return Err(Undecidable::new(
                "a mixed number whose fraction is not proper",
            ));
        };
        self.at = part.next;
        let mixed = Ast::Mixed {
            whole,
            numerator,
            denominator,
        };
        // The sign comes from the sign token, and the node keeps it outside the
        // whole part. `-0` is the integer zero, so a rule that reads the value
        // drops the minus of `-0 1/2` and grades minus one half as plus one half
        // (review round 3, finding #7).
        let value = if negative {
            Ast::Neg(Box::new(mixed))
        } else {
            mixed
        };
        // A mixed number is a primary, so it takes the postfix `%` as every
        // other primary does: `3 1/2%` is three and a half hundredths.
        Ok(Some(self.apply_percent(value)?))
    }

    /// Read the fraction that stands at the cursor, in either token shape.
    fn read_fraction_part(&self) -> Option<FractionPart> {
        let token = self.tokens.get(self.at)?;
        match &token.kind {
            Tok::Frac {
                numerator,
                denominator,
            } => Some(FractionPart {
                digits: digit_run_body(numerator).zip(digit_run_body(denominator)),
                next: self.at + 1,
                digit_run: false,
            }),
            Tok::Num(numerator) if token.space_before => {
                if self.peek_at(1) != Some(&Tok::Slash) {
                    return None;
                }
                let Some(Tok::Num(denominator)) = self.peek_at(2) else {
                    return None;
                };
                Some(FractionPart {
                    digits: Some((numerator.clone(), denominator.clone())),
                    next: self.at + 3,
                    digit_run: true,
                })
            }
            _ => None,
        }
    }
}
