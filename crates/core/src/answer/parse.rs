//! Recursive-descent parser for the decidable answer grammar (V1, V2).
//!
//! The grammar is `docs/reference/checker-1.0-spec.md` section 8.1 plus the interval
//! and inequality productions that `docs/plans/M2.md` fixes. Everything outside it is
//! [`Undecidable`]. The parser never panics and it never runs an unbounded search:
//! it reads each token once and it caps its own nesting.

use num_bigint::BigInt;
use num_traits::Zero;

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
            items.push(self.parse_expr()?);
        }
        Ok(Ast::Tuple(items))
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
        if !is_variable_name(&name) {
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
    /// Two rules, and both of them come from how a learner writes:
    ///
    /// - A number after a number is never a product. `2 3` is a typing slip, and
    ///   `9 R2` is a quotient with a remainder (spec section 8.3), not `9*R*2`.
    /// - A number glued to a name is a label: `R2`, `H1`, `x2` name one thing. A
    ///   space makes it a product, which is how `6 x 10**3` reads.
    fn check_implicit_number(&self, previous: Option<&Ast>) -> Result<(), Undecidable> {
        let previous_is_literal = matches!(
            previous,
            Some(Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. })
        );
        if previous_is_literal {
            return Err(Undecidable::new("two numbers stand side by side"));
        }
        let spaced = self.tokens.get(self.at).is_some_and(|t| t.space_before);
        if spaced {
            Ok(())
        } else {
            Err(Undecidable::new(
                "a number glued to a name reads as a label",
            ))
        }
    }

    /// Whether the cursor is on a token that can start a factor.
    fn starts_operand(&self) -> bool {
        matches!(self.peek(), Some(Tok::Num(_) | Tok::Ident(_) | Tok::LParen))
    }

    /// Take a spaced `x` that stands between two numbers, which means times.
    ///
    /// 27 authored corpus answers of 5 topics write the times sign as `x`
    /// (`6 x 10^3`, `2 x 2 x 3`). Every other `x` is the variable, so the reading
    /// asks for a space on both sides and a number literal on both sides
    /// (review finding #18).
    fn eat_times_letter(&mut self, previous: Option<&Ast>) -> bool {
        if !matches!(
            previous,
            Some(Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. })
        ) {
            return false;
        }
        let Some(token) = self.tokens.get(self.at) else {
            return false;
        };
        if !token.space_before || token.kind != Tok::Ident("x".to_string()) {
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

    /// Read a mixed number `a b/c` when the factors so far are exactly the whole part.
    ///
    /// The space in front of `b` is required, which is what tells `3 1/2` from `31/2`.
    ///
    /// The fractional part must be proper and plainly written: `0 < b < c`, no
    /// leading zero, and no three-digit numerator. A three-digit run after a space
    /// is the thousands group of the V4 table, so `1 000/3` and `1 200/300` are
    /// undecidable and never become a value the checker invented (finding #7).
    fn read_mixed_number(&mut self, factors: &[Ast]) -> Result<Option<Ast>, Undecidable> {
        let [only] = factors else {
            return Ok(None);
        };
        let Some(whole) = whole_number(only) else {
            return Ok(None);
        };
        let Some(Token {
            kind: Tok::Num(numerator),
            space_before: true,
        }) = self.tokens.get(self.at)
        else {
            return Ok(None);
        };
        if self.peek_at(1) != Some(&Tok::Slash) {
            return Ok(None);
        }
        let Some(Tok::Num(denominator)) = self.peek_at(2) else {
            return Ok(None);
        };
        if !is_plain_digit_run(numerator) || !is_plain_digit_run(denominator) {
            return Ok(None);
        }
        if numerator.chars().count() == 3 {
            return Ok(None);
        }
        let numerator = parse_integer(numerator)?;
        let denominator = parse_integer(denominator)?;
        if numerator.is_zero() || numerator >= denominator {
            return Ok(None);
        }
        self.at += 3;
        Ok(Some(Ast::Mixed {
            whole,
            numerator,
            denominator,
        }))
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
        self.apply_power(base)
    }

    /// Read the letters of a splittable run at the cursor.
    fn peek_letter_run(&self) -> Option<Vec<char>> {
        let Some(Tok::Ident(name)) = self.peek() else {
            return None;
        };
        letter_run(name)
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
        let base = Ast::Var(last.to_string());
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

    /// Turn an identifier into a function call, a constant, or a variable.
    fn parse_name(&mut self, name: &str) -> Result<Ast, Undecidable> {
        if FUNCTIONS.contains(&name) {
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
            let allowed = if name == "log" { 1..=2 } else { 1..=1 };
            if !allowed.contains(&args.len()) {
                return Err(Undecidable::new(
                    "a function call with the wrong count of arguments",
                ));
            }
            return Ok(Ast::Func(name.to_string(), args));
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
        let call = Ast::Func(name.to_string(), vec![argument]);
        Ok(match power {
            Some(exponent) => Ast::Pow(Box::new(call), exponent),
            None => call,
        })
    }

    /// Parse the bracket-free argument of a function.
    ///
    /// The argument is the juxtaposed chain of atoms with their powers, so
    /// `cos 2x` is `cos(2*x)` and `sin 3t^2` is `sin(3*t**2)`. The chain stops at
    /// `+`, `-`, `,`, `)`, `=`, `<`, `>`, and at an explicit `*` or `/`, because
    /// none of them starts a factor. 1.0 reads the five authored corpus answers
    /// of this shape the same way (review finding #3).
    fn parse_juxtaposed_argument(&mut self) -> Result<Ast, Undecidable> {
        self.nested(|parser| {
            let mut factors = vec![parser.parse_power()?];
            loop {
                if parser.eat_times_letter(factors.last()) {
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

/// Whether `name` is a name a value label may carry.
///
/// One letter, or a spelled Greek name. A function name is never a label.
fn is_variable_name(name: &str) -> bool {
    if FUNCTIONS.contains(&name) {
        return false;
    }
    GREEK_VARIABLES.contains(&name) || name.chars().count() == 1
}

/// Split a multi-letter run into its single-letter variables, or refuse it.
///
/// The run splits only when every letter is a [`RUN_LETTERS`] letter, the letters
/// differ from each other, and the run is at most [`MAX_RUN_LETTERS`] long. A
/// repeated letter is a spelling, not a product: a variable times itself is
/// written as a power. Everything else — a function name, a Greek name, a
/// differential (`dx`), a word (`yes`), a label (`HT`), an upper-case run
/// (`DNE`) — stays undecidable (C4).
fn letter_run(name: &str) -> Option<Vec<char>> {
    let letters: Vec<char> = name.chars().collect();
    if letters.len() < 2 || letters.len() > MAX_RUN_LETTERS {
        return None;
    }
    if FUNCTIONS.contains(&name) || GREEK_VARIABLES.contains(&name) || name == "pi" {
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
