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

mod atom;
mod build;
mod exponent;
mod term;

use build::{is_variable_name, make_quotient, simple_inequality};

use super::Undecidable;
use super::ast::{Ast, IneqOp};
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

    /// Parse the whole answer: a label, a quotient with a remainder, a relation,
    /// a bare tuple, or one value.
    fn parse_answer(&mut self) -> Result<Ast, Undecidable> {
        if let Some(var) = self.read_value_label() {
            let value = self.parse_answer()?;
            return Ok(Ast::Assign {
                var,
                value: Box::new(value),
            });
        }
        let first = self.parse_expr()?;
        if self.at_remainder_marker() {
            self.bump();
            let remainder = self.parse_expr()?;
            return Ok(Ast::Tuple(vec![first, remainder]));
        }
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

    /// Whether the cursor is on the marker of a quotient with a remainder (D-F3).
    ///
    /// `9 R2`, `9 R 2`, `9R2`, and `x + 2 remainder 3` all read into the tuple
    /// `(quotient, remainder)`, which is the authored spelling `(9, 2)`. The word
    /// `remainder` is always the marker. The letter `R` is the marker only
    /// between two number tokens, so `2R` and `R` stay the variable `R`, and
    /// `2 R x` stays the product. The lower-case `r` is never the marker: `9 r2`
    /// keeps its label refusal, and `9 r 2` keeps the product reading.
    ///
    /// A remainder that is not smaller than the divisor is not the concern of
    /// the grammar: no divisor is known here.
    pub(super) fn at_remainder_marker(&self) -> bool {
        let Some(Tok::Ident(name)) = self.peek() else {
            return false;
        };
        if name == "remainder" {
            return true;
        }
        if name != "R" {
            return false;
        }
        let before = self.at.checked_sub(1).and_then(|at| self.tokens.get(at));
        matches!(before.map(|token| &token.kind), Some(Tok::Num(_)))
            && matches!(self.peek_at(1), Some(Tok::Num(_)))
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
}
