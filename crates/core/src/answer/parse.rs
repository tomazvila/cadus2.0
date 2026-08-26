//! Recursive-descent parser for the decidable answer grammar (V1, V2).
//!
//! The grammar is `docs/reference/checker-1.0-spec.md` section 8.1 plus the interval
//! and inequality productions that `docs/plans/M2.md` fixes. Everything outside it is
//! [`Undecidable`]. The parser never panics and it never runs an unbounded search:
//! it reads each token once and it caps its own nesting.
//!
//! # The constructs of the V4 table are tokens
//!
//! Review round 3 (`docs/reviews/M2-review-3.md`) moves every LaTeX and glyph
//! construct into [`crate::answer::lexer`], and this module builds the tree from
//! the tokens. The four shapes the ruling names are:
//!
//! - a fraction: [`Tok::Frac`] becomes [`Ast::Fraction`] when both bodies are
//!   integer literals, and the quotient [`Ast::Div`] of the two bodies when they
//!   are not. One construct is one node, whatever whitespace its braces hold.
//! - a root: [`Tok::Sqrt`], [`Tok::Root`] and the name `sqrt` all become
//!   [`Ast::Sqrt`], so one value has one node.
//! - a percent: [`Tok::Percent`] divides the primary in front of it by 100, and
//!   the node it builds holds that primary and nothing else, so no `/` and no
//!   `**` beside it re-associates (findings #3, #4, #6).
//! - a mixed number: [`Ast::Mixed`] carries the magnitude of the whole part, and
//!   a negative mixed number is that node inside an [`Ast::Neg`]. The sign comes
//!   from the sign token, never from the integer value, so `-0 1/2` keeps its
//!   minus (finding #7).

use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use super::Undecidable;
use super::ast::{Ast, Const, IneqOp};
use super::lexer::{Tok, Token, lex};
use super::normalize::MAX_ANSWER_CHARS;

/// The functions the grammar knows (spec section 8.1, production `fn`).
const FUNCTIONS: [&str; 17] = [
    "sqrt", "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "sinh", "cosh",
    "tanh", "exp", "ln", "log", "abs",
];

/// The spelled Greek variable names the grammar knows.
const GREEK_VARIABLES: [&str; 4] = ["theta", "alpha", "beta", "lamda"];

/// The letters a multi-letter run splits into (review known item "multi-letter runs").
///
/// `3xy^2` is the product `3*x*y**2`, so a short run of these letters becomes one
/// variable per letter. The list leaves out the letters that carry another
/// meaning: `d` starts a differential (`dx`), `e` is Euler's number, `i` and `j`
/// name the imaginary unit and a unit vector, `l` and `o` are the shapes of `1`
/// and `0`. A run that holds any other letter stays undecidable, which is what
/// keeps the prose class (`yes`, `no`, `DNE`) out of the grammar (C4).
const RUN_LETTERS: [char; 20] = [
    'a', 'b', 'c', 'f', 'g', 'h', 'k', 'm', 'n', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y',
    'z',
];

/// The longest letter run the parser splits into single-letter variables.
///
/// A longer run is a word, not a product. The corpus writes no product of more
/// than three juxtaposed variables without an operator.
const MAX_RUN_LETTERS: usize = 3;

/// The largest literal exponent the grammar allows (1.0 `_MAX_EXPONENT`).
const MAX_EXPONENT: i64 = 1_000;

/// The largest production nesting the parser descends into.
///
/// The cap turns a hostile `((((…` into [`Undecidable`] instead of a stack overflow.
const MAX_DEPTH: usize = 96;

/// Parse one normalized source string into the answer AST.
///
/// # Errors
///
/// Returns [`Undecidable`] for every string outside the grammar, for an input longer
/// than [`MAX_ANSWER_CHARS`], and for an exponent outside the evaluation bound.
pub fn parse(source: &str) -> Result<Ast, Undecidable> {
    parse_with_functions(source, &[])
}

/// Parse one source string with extra function names admitted (M4 U1, V2).
///
/// The template `answer_expr` of M4 reads inside this grammar plus a short
/// arithmetic function set: `gcd`, `lcm`, `floor`, `ceiling`, `min`, `max`,
/// `factorial`, and `binomial` (`docs/plans/M4.md`, the fixed decision "one
/// grammar"). An extra name builds [`Ast::Func`] the way a whitelisted name
/// does, so [`crate::template::eval`] evaluates it exactly and erases it before
/// the answer string exists. The answer string therefore always lies in the
/// grammar [`parse`] alone reads.
///
/// An extra name takes one or two arguments, and it takes them in brackets. The
/// caller checks the exact count, because the count belongs to the function and
/// not to the grammar.
///
/// `extra` holds one name per function and no duplicate. A name that is already
/// a whitelisted function, a Greek variable, or `pi` changes nothing.
///
/// # Errors
///
/// Returns [`Undecidable`] for every string outside the grammar, for an input longer
/// than [`MAX_ANSWER_CHARS`], and for an exponent outside the evaluation bound.
pub fn parse_with_functions(source: &str, extra: &[&str]) -> Result<Ast, Undecidable> {
    if source.chars().count() > MAX_ANSWER_CHARS {
        return Err(Undecidable::new("the answer is longer than the input cap"));
    }
    let tokens = lex(source)?;
    if tokens.is_empty() {
        return Err(Undecidable::new("the answer is empty"));
    }
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        depth: 0,
        extra,
    };
    let ast = parser.parse_answer()?;
    if parser.at != tokens.len() {
        return Err(Undecidable::new("trailing text after the answer"));
    }
    Ok(ast)
}

/// The parser state: the token list and the read cursor.
struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    depth: usize,
    /// The extra function names of [`parse_with_functions`]. Empty for [`parse`].
    extra: &'a [&'a str],
}

impl Parser<'_> {
    /// Whether the name is a function of this parse.
    fn is_function(&self, name: &str) -> bool {
        FUNCTIONS.contains(&name) || self.extra.contains(&name)
    }

    /// The argument counts the named function takes.
    ///
    /// `log` takes a base as its second argument. An extra name takes one or two
    /// arguments; the caller of [`parse_with_functions`] checks the exact count.
    fn call_arity(&self, name: &str) -> std::ops::RangeInclusive<usize> {
        if name == "log" || self.extra.contains(&name) {
            1..=2
        } else {
            1..=1
        }
    }
}

/// The fraction that stands after a whole number, in either token shape.
struct FractionPart {
    /// The two digit runs of the fraction, as the answer writes them.
    ///
    /// `None` when a body of the [`Tok::Frac`] token is not one number literal,
    /// which is every `\frac` whose braces hold an expression.
    digits: Option<(String, String)>,
    /// The token index after the fraction.
    next: usize,
    /// True for the `b/c` spelling of three tokens, false for one [`Tok::Frac`].
    digit_run: bool,
}

impl Parser<'_> {
    /// Look at the next token without taking it.
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|t| &t.kind)
    }

    /// Look at the token `ahead` places after the cursor.
    fn peek_at(&self, ahead: usize) -> Option<&Tok> {
        self.tokens.get(self.at + ahead).map(|t| &t.kind)
    }

    /// Take the next token.
    fn bump(&mut self) {
        self.at += 1;
    }

    /// Take the next token when it is `want`, and report whether it was there.
    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Take the next token when it is `want`, or refuse the answer.
    fn expect(&mut self, want: &Tok, reason: &'static str) -> Result<(), Undecidable> {
        if self.eat(want) {
            Ok(())
        } else {
            Err(Undecidable::new(reason))
        }
    }

    /// Parse the token list that a construct carries, and require the whole list.
    ///
    /// `\frac{A}{B}` and `\sqrt{A}` carry their bodies as token lists, so the
    /// body is parsed here and never spliced back into the outer token stream.
    /// A body that holds no expression, or that leaves a token over, refuses the
    /// whole answer.
    fn parse_body(&self, tokens: &[Token], reason: &'static str) -> Result<Ast, Undecidable> {
        if tokens.is_empty() {
            return Err(Undecidable::new(reason));
        }
        if self.depth >= MAX_DEPTH {
            return Err(Undecidable::new("the answer nests too deeply"));
        }
        let mut inner = Parser {
            tokens,
            at: 0,
            depth: self.depth + 1,
            extra: self.extra,
        };
        let ast = inner.parse_expr()?;
        if inner.at != tokens.len() {
            return Err(Undecidable::new(reason));
        }
        Ok(ast)
    }

    /// Build the value of a [`Tok::Frac`] token from its two bodies.
    fn fraction_value(
        &self,
        numerator: &[Token],
        denominator: &[Token],
    ) -> Result<Ast, Undecidable> {
        let reason = "a fraction with a body the reader cannot read";
        let numerator = self.parse_body(numerator, reason)?;
        let denominator = self.parse_body(denominator, reason)?;
        make_quotient(numerator, denominator)
    }

    /// Run `body` one level deeper, or refuse an answer that nests too far.
    fn nested<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, Undecidable>,
    ) -> Result<T, Undecidable> {
        if self.depth >= MAX_DEPTH {
            return Err(Undecidable::new("the answer nests too deeply"));
        }
        self.depth += 1;
        let result = body(self);
        self.depth -= 1;
        result
    }

    /// Parse the whole answer: a label, a relation, a bare tuple, or one value.
    fn parse_answer(&mut self) -> Result<Ast, Undecidable> {
        if let Some(var) = self.read_value_label() {
            let value = self.parse_answer()?;
            return Ok(Ast::Assign {
                var,
                value: Box::new(value),
            });
        }
        let first = self.parse_expr()?;
        if let Some(op) = self.peek_comparison() {
            self.bump();
            return self.parse_relation(first, op);
        }
        if self.peek() != Some(&Tok::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(&Tok::Comma) {
            self.check_bare_comma_group()?;
            items.push(self.parse_expr()?);
        }
        Ok(Ast::Tuple(items))
    }

    /// Refuse a comma thousands group that the whole-answer rule did not strip.
    ///
    /// The V4 table strips `1,500` on a full match of the whole answer and
    /// nowhere else. A comma group that reaches the parser therefore stands in a
    /// longer answer — `1,500%` or `3 + 1,500` — where it is neither the grouped
    /// number nor a tuple of two values. The rule mirrors
    /// [`Parser::continues_a_space_group`], so the two separators of the V4
    /// table get one answer and `1 500%` and `1,500%` are both undecidable
    /// (review round 3, finding #6).
    ///
    /// A bracket makes the tuple explicit, so `(1,500)` keeps the 1.0 reading.
    /// The shape of a group is the 1.0 `_COMMA_GROUPS_RE` shape: one to three
    /// digits, the comma, and three digits with no space after the comma.
    fn check_bare_comma_group(&self) -> Result<(), Undecidable> {
        let before = self.at.checked_sub(2).and_then(|at| self.tokens.get(at));
        let Some(Tok::Num(left)) = before.map(|token| &token.kind) else {
            return Ok(());
        };
        let Some(token) = self.tokens.get(self.at) else {
            return Ok(());
        };
        let Tok::Num(right) = &token.kind else {
            return Ok(());
        };
        if token.space_before {
            return Ok(());
        }
        let plain = |text: &str| text.chars().all(|c| c.is_ascii_digit());
        if plain(left)
            && (1..=3).contains(&left.chars().count())
            && plain(right)
            && right.chars().count() == 3
        {
            return Err(Undecidable::new(
                "a comma-grouped number stands in a longer answer",
            ));
        }
        Ok(())
    }

    /// Read a leading `<var> =` label and return the variable name.
    ///
    /// The label stands at the start of the answer and nowhere else, so a second
    /// `=` leaves text after the answer and the parser refuses the whole string.
    /// The name is one letter or a spelled Greek name; every other name in front
    /// of an `=` is an equation, which is outside the grammar (review finding #2).
    fn read_value_label(&mut self) -> Option<String> {
        if self.at != 0 || self.peek_at(1) != Some(&Tok::Eq) {
            return None;
        }
        let Some(Tok::Ident(name)) = self.peek() else {
            return None;
        };
        let name = name.clone();
        if !is_variable_name(&name, self.extra) {
            return None;
        }
        self.at += 2;
        Some(name)
    }

    /// Read a comparison operator at the cursor.
    fn peek_comparison(&self) -> Option<IneqOp> {
        match self.peek()? {
            Tok::Lt => Some(IneqOp::Lt),
            Tok::Le => Some(IneqOp::Le),
            Tok::Gt => Some(IneqOp::Gt),
            Tok::Ge => Some(IneqOp::Ge),
            _ => None,
        }
    }

    /// Parse the rest of an inequality after its first operator.
    fn parse_relation(&mut self, left: Ast, op: IneqOp) -> Result<Ast, Undecidable> {
        let middle = self.parse_expr()?;
        let Some(second) = self.peek_comparison() else {
            return simple_inequality(left, op, middle);
        };
        self.bump();
        let right = self.parse_expr()?;
        let Ast::Var(var) = middle else {
            return Err(Undecidable::new(
                "a chained inequality needs one variable in the middle",
            ));
        };
        match (op, second) {
            (IneqOp::Lt | IneqOp::Le, IneqOp::Lt | IneqOp::Le) => Ok(Ast::Chain {
                lo: Box::new(left),
                lo_closed: op == IneqOp::Le,
                var,
                hi_closed: second == IneqOp::Le,
                hi: Box::new(right),
            }),
            (IneqOp::Gt | IneqOp::Ge, IneqOp::Gt | IneqOp::Ge) => Ok(Ast::Chain {
                lo: Box::new(right),
                lo_closed: second == IneqOp::Ge,
                var,
                hi_closed: op == IneqOp::Ge,
                hi: Box::new(left),
            }),
            _ => Err(Undecidable::new(
                "a chained inequality points in two directions",
            )),
        }
    }

    /// Parse a sum: `term (('+' | '-') term)*`.
    fn parse_expr(&mut self) -> Result<Ast, Undecidable> {
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
    fn check_implicit_number(&self, previous: Option<&Ast>) -> Result<(), Undecidable> {
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
    fn starts_operand(&self) -> bool {
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
    fn eat_times_letter(&mut self, previous: Option<&Ast>) -> bool {
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
    fn read_mixed_number(&mut self, factors: &[Ast]) -> Result<Option<Ast>, Undecidable> {
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

    /// Parse a sign chain in front of a power.
    fn parse_unary(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            if parser.eat(&Tok::Minus) {
                return Ok(Ast::Neg(Box::new(parser.parse_unary()?)));
            }
            if parser.eat(&Tok::Plus) {
                return parser.parse_unary();
            }
            parser.parse_power()
        })
    }

    /// Parse an atom and at most one integer power.
    ///
    /// The one base with a free exponent is `e`: `e**t` is the whitelisted function
    /// `exp(t)`, which the grammar holds exactly. Every other base takes an integer
    /// exponent, because `Ast::Pow` carries an integer and nothing else (D6).
    fn parse_power(&mut self) -> Result<Ast, Undecidable> {
        if let Some(letters) = self.peek_letter_run() {
            self.bump();
            return self.finish_letter_run(&letters);
        }
        let base = self.parse_atom()?;
        let base = self.apply_percent(base)?;
        self.apply_power(base)
    }

    /// Read the postfix `%` after a primary, and divide that primary by 100.
    ///
    /// The percent binds to the primary in front of it and to nothing else
    /// (`docs/plans/M2.md`, round 1 finding #17). The node holds that primary,
    /// so `15/30%` is `15/(30/100)` = 50 and `4%^2` is `(4/100)^2`. Round 2
    /// spliced the text `(n)/100` into the source instead, and the `/100` then
    /// bound to the operator beside it: `15/30%` became `(15/30)/100`, which is
    /// a hundredth of a hundredth of the value the learner wrote (review round
    /// 3, findings #3, #4).
    ///
    /// One primary takes one percent. `50%%` is a slip, not a value, so the
    /// second sign refuses the answer.
    fn apply_percent(&mut self, value: Ast) -> Result<Ast, Undecidable> {
        if !self.eat(&Tok::Percent) {
            return Ok(value);
        }
        if self.peek() == Some(&Tok::Percent) {
            return Err(Undecidable::new("two percent signs on one number"));
        }
        make_quotient(value, Ast::Integer(BigInt::from(100)))
    }

    /// Read the letters of a splittable run at the cursor.
    fn peek_letter_run(&self) -> Option<Vec<char>> {
        let Some(Tok::Ident(name)) = self.peek() else {
            return None;
        };
        letter_run(name, self.extra)
    }

    /// Build the product of a split letter run. The power binds to the last letter.
    ///
    /// `3xy^2` is `3*x*y**2`, so the exponent belongs to `y` alone.
    fn finish_letter_run(&mut self, letters: &[char]) -> Result<Ast, Undecidable> {
        let Some((last, leading)) = letters.split_last() else {
            return Err(Undecidable::new(
                "a name that is not a function or variable",
            ));
        };
        let mut factors: Vec<Ast> = leading
            .iter()
            .map(|letter| Ast::Var(letter.to_string()))
            .collect();
        let base = self.apply_percent(Ast::Var(last.to_string()))?;
        factors.push(self.apply_power(base)?);
        Ok(collapse(factors, Ast::Mul))
    }

    /// Read at most one power after an atom the parser already took.
    fn apply_power(&mut self, base: Ast) -> Result<Ast, Undecidable> {
        if !self.eat(&Tok::Pow) {
            return Ok(base);
        }
        if base == Ast::Const(Const::E) {
            let exponent = self.parse_unary()?;
            return Ok(Ast::Func("exp".to_string(), vec![exponent]));
        }
        let exponent = self.parse_exponent()?;
        if self.peek() == Some(&Tok::Pow) {
            return Err(Undecidable::new("a tower of powers"));
        }
        Ok(Ast::Pow(Box::new(base), exponent))
    }

    /// Parse the exponent of a power. The grammar allows an integer literal only.
    fn parse_exponent(&mut self) -> Result<i64, Undecidable> {
        let parenthesized = self.eat(&Tok::LParen);
        let mut negative = false;
        loop {
            if self.eat(&Tok::Minus) {
                negative = !negative;
            } else if self.eat(&Tok::Plus) {
            } else {
                break;
            }
        }
        let Some(Tok::Num(text)) = self.peek() else {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        };
        if text.contains('.') {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        }
        let magnitude: i64 = text
            .parse()
            .map_err(|_| Undecidable::new("an exponent outside the evaluation bound"))?;
        self.bump();
        // `2^50%` writes the exponent 50/100, and [`Ast::Pow`] carries a whole
        // number and nothing else (D6). The percent binds tighter than the
        // power, so this is the same refusal that `2^0.5` gets, and the answer
        // never takes the second reading `(2^50)/100` (review round 3, #3, #4).
        if self.peek() == Some(&Tok::Percent) {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        }
        if parenthesized && !self.eat(&Tok::RParen) {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        }
        if magnitude > MAX_EXPONENT {
            return Err(Undecidable::new("an exponent outside the evaluation bound"));
        }
        Ok(if negative { -magnitude } else { magnitude })
    }

    /// Parse one atom: a literal, a name, a bracketed group, or a collection.
    fn parse_atom(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            let Some(token) = parser.tokens.get(parser.at) else {
                return Err(Undecidable::new("the answer ends where a value belongs"));
            };
            match &token.kind {
                Tok::Num(text) => {
                    let text = text.clone();
                    parser.bump();
                    parse_number(&text)
                }
                Tok::Frac {
                    numerator,
                    denominator,
                } => {
                    let numerator = numerator.clone();
                    let denominator = denominator.clone();
                    parser.bump();
                    parser.fraction_value(&numerator, &denominator)
                }
                Tok::Sqrt(body) => {
                    let body = body.clone();
                    parser.bump();
                    let argument = parser.parse_body(&body, "a root with no argument")?;
                    Ok(Ast::Sqrt(Box::new(argument)))
                }
                Tok::Root => {
                    parser.bump();
                    parser.parse_root_glyph()
                }
                Tok::Ident(name) => {
                    let name = name.clone();
                    parser.bump();
                    parser.parse_name(&name)
                }
                Tok::LParen => parser.parse_paren_group(),
                Tok::LBrack => parser.parse_bracket_group(),
                Tok::LBrace => {
                    parser.bump();
                    let items = parser.parse_items(&Tok::RBrace, "a set with no closing brace")?;
                    if items.is_empty() {
                        return Err(Undecidable::new("an empty set"));
                    }
                    Ok(Ast::Set(items))
                }
                _ => Err(Undecidable::new("a symbol where a value belongs")),
            }
        })
    }

    /// Read the one primary that the radical glyph `√` takes.
    ///
    /// 1.0 gives the glyph a bracketed group, a number, or a name
    /// (`sympy_check.py:153-157`), and 2.0 keeps that reading, so `15√3` is
    /// `15*sqrt(3)` and `√x^2` is `sqrt(x)^2`. A function name after the glyph
    /// is two function names in a row, which the grammar does not read.
    fn parse_root_glyph(&mut self) -> Result<Ast, Undecidable> {
        let no_argument = Undecidable::new("a root with no argument");
        let Some(token) = self.tokens.get(self.at) else {
            return Err(no_argument);
        };
        let argument = match &token.kind {
            Tok::LParen => self.parse_paren_group()?,
            Tok::Num(text) => {
                let text = text.clone();
                self.bump();
                parse_number(&text)?
            }
            Tok::Frac {
                numerator,
                denominator,
            } => {
                let numerator = numerator.clone();
                let denominator = denominator.clone();
                self.bump();
                self.fraction_value(&numerator, &denominator)?
            }
            Tok::Ident(name) if !self.is_function(name) => {
                let name = name.clone();
                self.bump();
                match letter_run(&name, self.extra) {
                    Some(letters) => collapse(
                        letters
                            .iter()
                            .map(|letter| Ast::Var(letter.to_string()))
                            .collect(),
                        Ast::Mul,
                    ),
                    None => self.parse_name(&name)?,
                }
            }
            _ => return Err(no_argument),
        };
        Ok(Ast::Sqrt(Box::new(argument)))
    }

    /// Turn an identifier into a function call, a constant, or a variable.
    fn parse_name(&mut self, name: &str) -> Result<Ast, Undecidable> {
        if self.is_function(name) {
            return self.parse_call(name);
        }
        if name == "pi" {
            return Ok(Ast::Const(Const::Pi));
        }
        if name == "e" || name == "E" {
            return Ok(Ast::Const(Const::E));
        }
        if GREEK_VARIABLES.contains(&name) {
            return Ok(Ast::Var(name.to_string()));
        }
        if name.chars().count() == 1 {
            return Ok(Ast::Var(name.to_string()));
        }
        Err(Undecidable::new(
            "a name that is not a function or variable",
        ))
    }

    /// Parse the argument of a whitelisted function, with or without brackets.
    fn parse_call(&mut self, name: &str) -> Result<Ast, Undecidable> {
        if self.eat(&Tok::LParen) {
            let args = self.parse_items(&Tok::RParen, "a function call with no closing bracket")?;
            let allowed = self.call_arity(name);
            if !allowed.contains(&args.len()) {
                return Err(Undecidable::new(
                    "a function call with the wrong count of arguments",
                ));
            }
            return Ok(make_call(name, args));
        }
        // `sec**2 x` is the house spelling of `sec(x)**2` (spec section 8.2).
        let power = if self.eat(&Tok::Pow) {
            Some(self.parse_exponent()?)
        } else {
            None
        };
        if !self.starts_operand() {
            return Err(Undecidable::new("a function name with no argument"));
        }
        let argument = self.parse_juxtaposed_argument()?;
        let call = make_call(name, vec![argument]);
        Ok(match power {
            Some(exponent) => Ast::Pow(Box::new(call), exponent),
            None => call,
        })
    }

    /// Parse the bracket-free argument of a function.
    ///
    /// The argument is the juxtaposed chain of atoms with their powers, so
    /// `cos 2x` is `cos(2*x)` and `sin 3t^2` is `sin(3*t**2)`. The chain runs
    /// through an explicit `*` as well, so `cos 2*x` is `cos(2*x)` and not
    /// `x*cos(2)`: the two spellings are one answer, and 1.0 reads both of them
    /// as `cos(2*x)` (review round 2, findings #4 and #15).
    ///
    /// The chain stops at `/`, `+`, `-`, `,`, `)`, `=`, `<`, `>`, and at a
    /// function. The stop at `/` is the round 1 ruling that keeps `sqrt 2/2` at
    /// `sqrt(2)/2`; the stop at a function is the round 3 ruling of finding #5.
    fn parse_juxtaposed_argument(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            let mut factors = vec![parser.parse_power()?];
            loop {
                if parser.stops_the_argument_chain() {
                    break;
                }
                if let Some(mixed) = parser.read_mixed_number(&factors)? {
                    factors = vec![mixed];
                    continue;
                }
                if parser.eat_times_letter(factors.last()) {
                    factors.push(parser.parse_power()?);
                    continue;
                }
                if parser.eat(&Tok::Star) {
                    factors.push(parser.parse_power()?);
                    continue;
                }
                if !parser.starts_operand() {
                    break;
                }
                if matches!(parser.peek(), Some(Tok::Num(_))) {
                    parser.check_implicit_number(factors.last())?;
                }
                factors.push(parser.parse_power()?);
            }
            Ok(collapse(factors, Ast::Mul))
        })
    }

    /// Whether the next factor of a bracket-free argument is a function.
    ///
    /// A bracket-free argument ends where the next function starts, so
    /// `sec x tan x` is `sec(x)*tan(x)` and `2 sin x cos x` is
    /// `2*sin(x)*cos(x)`. Round 2 continued the chain over every operand, so the
    /// authored `sec x tan x` meant `sec(x*tan(x))`: the correct learner answer
    /// `sec(x)tan(x)` was graded wrong and the meaningless `sec(x tan x)` was
    /// graded correct on three authored `derivatives-trig` answers (review round
    /// 3, finding #5).
    ///
    /// The test reads through one explicit `*`, because the chain runs through
    /// `*` (round 2, findings #4, #15). Without that, `sec x * tan x` and
    /// `sec x tan x` would be two values of one answer.
    ///
    /// A function is a name of [`FUNCTIONS`], a `\sqrt{…}` token, or the glyph
    /// `√`. The last two are the name `sqrt` in another spelling, so all three
    /// stop the chain in the same place.
    fn stops_the_argument_chain(&self) -> bool {
        let ahead = match self.peek() {
            Some(Tok::Star) => self.peek_at(1),
            other => other,
        };
        match ahead {
            Some(Tok::Ident(name)) => self.is_function(name),
            Some(Tok::Sqrt(_) | Tok::Root) => true,
            _ => false,
        }
    }

    /// Parse `( … )`: a group, an ordered tuple, or the open end of an interval.
    fn parse_paren_group(&mut self) -> Result<Ast, Undecidable> {
        self.expect(&Tok::LParen, "a group with no opening bracket")?;
        let mut items = vec![self.parse_expr()?];
        while self.eat(&Tok::Comma) {
            items.push(self.parse_expr()?);
        }
        if self.eat(&Tok::RBrack) {
            return make_interval(items, false, true);
        }
        self.expect(&Tok::RParen, "a group with no closing bracket")?;
        match items.len() {
            1 => items
                .pop()
                .ok_or_else(|| Undecidable::new("an empty group")),
            _ => Ok(Ast::Tuple(items)),
        }
    }

    /// Parse `[ … ]`: an ordered list, or the closed end of an interval.
    fn parse_bracket_group(&mut self) -> Result<Ast, Undecidable> {
        self.expect(&Tok::LBrack, "a list with no opening bracket")?;
        let mut items = vec![self.parse_expr()?];
        while self.eat(&Tok::Comma) {
            items.push(self.parse_expr()?);
        }
        if self.eat(&Tok::RParen) {
            return make_interval(items, true, false);
        }
        self.expect(&Tok::RBrack, "a list with no closing bracket")?;
        Ok(Ast::List(items))
    }

    /// Parse a comma-separated body up to `close`.
    fn parse_items(&mut self, close: &Tok, reason: &'static str) -> Result<Vec<Ast>, Undecidable> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Ok(items);
        }
        items.push(self.parse_expr()?);
        while self.eat(&Tok::Comma) {
            items.push(self.parse_expr()?);
        }
        self.expect(close, reason)?;
        Ok(items)
    }
}

/// Whether a token is the single letter `x` or `X` that stands for times.
fn is_times_letter(kind: &Tok) -> bool {
    match kind {
        Tok::Ident(name) => name == "x" || name == "X",
        _ => false,
    }
}

/// Whether `name` is a name a value label may carry.
///
/// One letter, or a spelled Greek name. A function name is never a label, and
/// `extra` holds the function names of [`parse_with_functions`].
fn is_variable_name(name: &str, extra: &[&str]) -> bool {
    if FUNCTIONS.contains(&name) || extra.contains(&name) {
        return false;
    }
    GREEK_VARIABLES.contains(&name) || name.chars().count() == 1
}

/// Split a multi-letter run into its single-letter variables, or refuse it.
///
/// The run splits only when every letter is a [`RUN_LETTERS`] letter, the letters
/// differ from each other, the run is not a function name of this parse (`extra`
/// holds the extra names, so `gcd` stays one function and never becomes the
/// product `g*c*d`), and the run is at most [`MAX_RUN_LETTERS`] long. A
/// repeated letter is a spelling, not a product: a variable times itself is
/// written as a power. Everything else — a function name, a Greek name, a
/// differential (`dx`), a word (`yes`), a label (`HT`), an upper-case run
/// (`DNE`) — stays undecidable (C4).
fn letter_run(name: &str, extra: &[&str]) -> Option<Vec<char>> {
    let letters: Vec<char> = name.chars().collect();
    if letters.len() < 2 || letters.len() > MAX_RUN_LETTERS {
        return None;
    }
    if FUNCTIONS.contains(&name)
        || extra.contains(&name)
        || GREEK_VARIABLES.contains(&name)
        || name == "pi"
    {
        return None;
    }
    for (index, letter) in letters.iter().enumerate() {
        if !RUN_LETTERS.contains(letter) {
            return None;
        }
        if letters.get(index + 1..)?.contains(letter) {
            return None;
        }
    }
    Some(letters)
}

/// Read the fraction part as a proper fraction, or refuse it.
///
/// The checks are the same for every mixed-number spelling: two plain digit
/// runs, and `0 < b < c`. The digit-run spelling refuses a three-digit numerator
/// as well, because a three-digit run after a space is a thousands group.
fn proper_fraction_part(part: &FractionPart) -> Option<(BigInt, BigInt)> {
    let (numerator, denominator) = part.digits.as_ref()?;
    if !is_plain_digit_run(numerator) || !is_plain_digit_run(denominator) {
        return None;
    }
    if part.digit_run && numerator.chars().count() == 3 {
        return None;
    }
    let numerator = numerator.parse::<BigInt>().ok()?;
    let denominator = denominator.parse::<BigInt>().ok()?;
    if numerator.is_zero() || numerator >= denominator {
        return None;
    }
    Some((numerator, denominator))
}

/// Read the digit run of one `\frac` brace body, when the body is exactly one.
///
/// The body reaches the parser as a token list, so whitespace inside the braces
/// changes nothing: `\frac{ 1 }{2}` and `\frac{1}{2}` hold the same one token.
/// Round 2 tested the brace text instead, and one space there turned the mixed
/// number `2\frac{ 1}{2}` into the product 1 (review round 3, findings #1, #2).
fn digit_run_body(tokens: &[Token]) -> Option<String> {
    match tokens {
        [only] => match &only.kind {
            Tok::Num(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Read the sign token and the magnitude of a whole-number literal.
///
/// The sign comes from the [`Ast::Neg`] node that the sign token built, and
/// never from the integer value. `-0` is the integer zero, so a rule that reads
/// the value alone drops the minus of `-0 1/2` and grades minus one half as plus
/// one half (review round 3, finding #7).
fn signed_whole(node: &Ast) -> Option<(bool, BigInt)> {
    match node {
        Ast::Integer(value) => Some((value.is_negative(), value.abs())),
        Ast::Neg(inner) => signed_whole(inner).map(|(negative, magnitude)| (!negative, magnitude)),
        _ => None,
    }
}

/// Whether the node is a number literal, with or without a leading sign.
fn is_numeric_literal(node: &Ast) -> bool {
    match node {
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. } => true,
        Ast::Neg(inner) => is_numeric_literal(inner),
        _ => false,
    }
}

/// Whether the text is a digit run that carries no grouping and no leading zero.
fn is_plain_digit_run(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    if first == '0' && chars.clone().next().is_some() {
        return false;
    }
    chars.all(|c| c.is_ascii_digit())
}

/// Build an interval from the two ends of a mixed bracket pair.
fn make_interval(
    mut items: Vec<Ast>,
    lo_closed: bool,
    hi_closed: bool,
) -> Result<Ast, Undecidable> {
    if items.len() != 2 {
        return Err(Undecidable::new("an interval that has no two ends"));
    }
    let hi = items
        .pop()
        .ok_or_else(|| Undecidable::new("an interval with no upper end"))?;
    let lo = items
        .pop()
        .ok_or_else(|| Undecidable::new("an interval with no lower end"))?;
    Ok(Ast::Interval {
        lo: Box::new(lo),
        hi: Box::new(hi),
        lo_closed,
        hi_closed,
    })
}

/// Build a simple inequality, with the variable moved to the left.
fn simple_inequality(left: Ast, op: IneqOp, right: Ast) -> Result<Ast, Undecidable> {
    match (left, right) {
        (Ast::Var(_), Ast::Var(_)) => Err(Undecidable::new("an inequality between two variables")),
        (Ast::Var(var), bound) => Ok(Ast::Ineq {
            var,
            op,
            bound: Box::new(bound),
        }),
        (bound, Ast::Var(var)) => Ok(Ast::Ineq {
            var,
            op: op.flipped(),
            bound: Box::new(bound),
        }),
        _ => Err(Undecidable::new("an inequality with no bare variable")),
    }
}

/// Fold a one-element list into its element, and a longer one into `build`.
fn collapse(mut parts: Vec<Ast>, build: fn(Vec<Ast>) -> Ast) -> Ast {
    if parts.len() == 1 {
        match parts.pop() {
            Some(single) => single,
            None => build(parts),
        }
    } else {
        build(parts)
    }
}

/// Build a quotient, folding an integer over an integer into a fraction.
fn make_quotient(dividend: Ast, divisor: Ast) -> Result<Ast, Undecidable> {
    if let (Some(numerator), Some(denominator)) = (whole_number(&dividend), whole_number(&divisor))
    {
        if denominator.is_zero() {
            return Err(Undecidable::new("a fraction with a zero denominator"));
        }
        let negative = denominator.sign() == num_bigint::Sign::Minus;
        let (numerator, denominator) = if negative {
            (-numerator, -denominator)
        } else {
            (numerator, denominator)
        };
        return Ok(Ast::Fraction {
            numerator,
            denominator,
        });
    }
    if whole_number(&divisor).is_some_and(|value| value.is_zero()) {
        return Err(Undecidable::new("a quotient with a zero divisor"));
    }
    Ok(Ast::Div(Box::new(dividend), Box::new(divisor)))
}

/// Build a function application, with one node for every square root.
///
/// `sqrt(2)`, `\sqrt{2}` and `√2` are one value, so all three build
/// [`Ast::Sqrt`] and the canonicalizer meets one shape (review round 3, the
/// structural ruling).
fn make_call(name: &str, mut args: Vec<Ast>) -> Ast {
    if name == "sqrt"
        && args.len() == 1
        && let Some(argument) = args.pop()
    {
        return Ast::Sqrt(Box::new(argument));
    }
    Ast::Func(name.to_string(), args)
}

/// Read the value of a node that is a signed integer literal, if it is one.
fn whole_number(node: &Ast) -> Option<BigInt> {
    match node {
        Ast::Integer(value) => Some(value.clone()),
        Ast::Neg(inner) => whole_number(inner).map(|value| -value),
        _ => None,
    }
}

/// Turn a number literal into an integer or an exact decimal.
fn parse_number(text: &str) -> Result<Ast, Undecidable> {
    match text.split_once('.') {
        None => Ok(Ast::Integer(parse_integer(text)?)),
        Some((whole, fraction)) => {
            let scale = u32::try_from(fraction.chars().count())
                .map_err(|_| Undecidable::new("a decimal with too many digits"))?;
            let digits = format!("{whole}{fraction}");
            Ok(Ast::Decimal {
                mantissa: parse_integer(&digits)?,
                scale,
            })
        }
    }
}

/// Parse a run of decimal digits into a big integer.
fn parse_integer(text: &str) -> Result<BigInt, Undecidable> {
    if text.is_empty() {
        return Ok(BigInt::zero());
    }
    text.parse::<BigInt>()
        .map_err(|_| Undecidable::new("a number the reader cannot read"))
}
