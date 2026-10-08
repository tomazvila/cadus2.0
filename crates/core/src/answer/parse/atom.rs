//! The unary, power, and atom productions, and the bracketed groups.

use num_bigint::BigInt;

use super::build::{collapse, letter_run, make_call, make_interval, make_quotient, parse_number};
use super::exponent::{Exponent, raise};
use super::{GREEK_VARIABLES, Parser};
use crate::answer::Undecidable;
use crate::answer::ast::{Ast, Const};
use crate::answer::lexer::Tok;

/// The name of the call that holds a root with an index above 2.
pub(crate) const ROOT_CALL: &str = "root";

/// Whether `name` is a spelling of a root written as a function call.
fn is_root_call(name: &str) -> bool {
    name == "root" || name == "nthroot" || name == "cbrt"
}

/// Check the index of a root: a whole number from 2 to 9.
fn root_index(index: i64) -> Result<i64, Undecidable> {
    if (2..=9).contains(&index) {
        Ok(index)
    } else {
        Err(Undecidable::new("a root index outside 2 to 9"))
    }
}

/// Build the root node of an index: the index 2 is the square root.
///
/// A root with a larger index is the call `root(index, radicand)`, which the
/// canonicalizer reads as `radicand^(1/index)`. The call keeps the radical form
/// of an answer apart from the fractional exponent.
fn make_root(radicand: Ast, index: i64) -> Ast {
    if index == 2 {
        Ast::Sqrt(Box::new(radicand))
    } else {
        Ast::Func(
            ROOT_CALL.to_string(),
            vec![Ast::Integer(BigInt::from(index)), radicand],
        )
    }
}

/// Whether the name is a differential: `d` and one lower-case letter, as in `dx`.
///
/// The differential is one symbol. It equals only itself, so `dy/dx` and `3x^2 dx`
/// compare as written and `dx` never reads as the product of `d` and `x`.
fn is_differential(name: &str) -> bool {
    let mut letters = name.chars();
    matches!(
        (letters.next(), letters.next(), letters.next()),
        (Some('d'), Some(second), None) if second.is_ascii_lowercase()
    )
}

/// The value of `n!` for a whole number `n` from 0 to 20, as the integer it is.
fn factorial(text: &str) -> Result<Ast, Undecidable> {
    let count: u32 = text
        .parse()
        .ok()
        .filter(|count| *count <= 20)
        .ok_or_else(|| Undecidable::new("a factorial of a number outside 0 to 20"))?;
    Ok(Ast::Integer((1..=count).map(BigInt::from).product()))
}

/// The plain inverse function that an inverse reciprocal function is built on.
fn inverse_reciprocal(name: &str) -> Option<&'static str> {
    match name {
        "asec" => Some("acos"),
        "acsc" => Some("asin"),
        _ => None,
    }
}

impl Parser<'_> {
    /// Parse a sign chain in front of a power.
    pub(super) fn parse_unary(&mut self) -> Result<Ast, Undecidable> {
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

    /// Parse an atom and at most one power.
    ///
    /// The one base with a free exponent is `e`: `e**t` is the whitelisted function
    /// `exp(t)`, which the grammar holds exactly. Every other base takes a whole
    /// exponent or a bracketed rational one, `x^(1/2)` (D-F3).
    fn parse_power(&mut self) -> Result<Ast, Undecidable> {
        if let Some((leading, last)) = self.peek_letter_run() {
            self.bump();
            return self.finish_letter_run(&leading, last);
        }
        if let Some(pieces) = self.peek_pieces() {
            self.bump();
            return self.finish_pieces(&pieces);
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
    pub(super) fn apply_percent(&mut self, value: Ast) -> Result<Ast, Undecidable> {
        if !self.eat(&Tok::Percent) {
            return Ok(value);
        }
        if self.peek() == Some(&Tok::Percent) {
            return Err(Undecidable::new("two percent signs on one number"));
        }
        make_quotient(value, Ast::Integer(BigInt::from(100)))
    }

    /// Read the letters of a splittable run at the cursor.
    fn peek_letter_run(&self) -> Option<(Vec<char>, char)> {
        let Some(Tok::Ident(name)) = self.peek() else {
            return None;
        };
        letter_run(name, self.extra)
    }

    /// Build the product of a split letter run. The power binds to the last letter.
    ///
    /// `3xy^2` is `3*x*y**2`, so the exponent belongs to `y` alone.
    fn finish_letter_run(&mut self, leading: &[char], last: char) -> Result<Ast, Undecidable> {
        let mut factors: Vec<Ast> = leading
            .iter()
            .map(|letter| Ast::Var(letter.to_string()))
            .collect();
        let base = self.apply_percent(Ast::Var(last.to_string()))?;
        factors.push(self.apply_power(base)?);
        Ok(collapse(factors, Ast::Mul))
    }

    /// Read at most one power after an atom the parser already took.
    pub(super) fn apply_power(&mut self, base: Ast) -> Result<Ast, Undecidable> {
        if !self.eat(&Tok::Pow) {
            return Ok(base);
        }
        if base == Ast::Const(Const::E) {
            let exponent = self.parse_unary()?;
            return Ok(Ast::Func("exp".to_string(), vec![exponent]));
        }
        let saved = self.at;
        let exponent = match self.parse_exponent() {
            Ok(exponent) => exponent,
            Err(reason) => return self.variable_power(base, saved, reason),
        };
        if self.peek() == Some(&Tok::Pow) {
            return Err(Undecidable::new("a tower of powers"));
        }
        Ok(raise(base, exponent))
    }

    /// Read a power whose exponent holds a variable: `3^t`, `1.005^(12t)`,
    /// `4^(n-1)`. The node is the call `pow(base, exponent)`, which the
    /// canonical form keeps whole and the `function` contract compares by value.
    /// An exponent with no variable keeps the refusal it had.
    fn variable_power(
        &mut self,
        base: Ast,
        saved: usize,
        reason: Undecidable,
    ) -> Result<Ast, Undecidable> {
        self.at = saved;
        // The template grammar (extra function names) keeps its refusal.
        if !self.extra.is_empty() {
            return Err(reason);
        }
        let Ok(exponent) = self.free_exponent() else {
            return Err(reason);
        };
        if crate::answer::evalf::free_vars(&exponent).is_empty() || self.peek() == Some(&Tok::Pow) {
            return Err(reason);
        }
        Ok(Ast::Func("pow".to_string(), vec![base, exponent]))
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
                    if parser.eat(&Tok::Bang) {
                        return factorial(&text);
                    }
                    Ok(parse_number(&text))
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
                Tok::NthRoot(index, body) => {
                    let index = index.clone();
                    let body = body.clone();
                    parser.bump();
                    let argument = parser.parse_body(&body, "a root with no argument")?;
                    let index = root_index(index.parse().unwrap_or(0))?;
                    Ok(make_root(argument, index))
                }
                Tok::Root => {
                    parser.bump();
                    let argument = parser.parse_root_glyph()?;
                    Ok(Ast::Sqrt(Box::new(argument)))
                }
                Tok::IndexedRoot(index) => {
                    let index = *index;
                    parser.bump();
                    let argument = parser.parse_root_glyph()?;
                    Ok(make_root(argument, index))
                }
                Tok::Ident(name)
                    if is_root_call(name) && parser.peek_at(1) == Some(&Tok::LParen) =>
                {
                    let name = name.clone();
                    parser.bump();
                    parser.parse_root_call(&name)
                }
                Tok::Ident(word)
                    if word == "not"
                        && matches!(parser.peek_at(1), Some(Tok::Ident(next))
                            if next.chars().count() == 1 && next.chars().all(|c| c.is_ascii_lowercase())) =>
                {
                    // `not x` is the complement `x'`.
                    let Some(Tok::Ident(next)) = parser.peek_at(1) else {
                        return Err(Undecidable::new("a symbol where a value belongs"));
                    };
                    let complement = format!("{next}'");
                    parser.at += 2;
                    Ok(Ast::Var(complement))
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
                    Ok(Ast::Set(items))
                }
                Tok::Unit(_) => Err(Undecidable::new("a unit inside an expression")),
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
                parse_number(&text)
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
                    Some((leading, last)) => collapse(
                        leading
                            .iter()
                            .chain(std::iter::once(&last))
                            .map(|letter| Ast::Var(letter.to_string()))
                            .collect(),
                        Ast::Mul,
                    ),
                    None => self.parse_name(&name)?,
                }
            }
            _ => return Err(no_argument),
        };
        Ok(argument)
    }

    /// Read `root(n, a)` or `cbrt(a)`. The caller took the name and saw the bracket.
    ///
    /// The index of `root` is one whole-number literal from 2 to 9, and `cbrt`
    /// is the index 3.
    fn parse_root_call(&mut self, name: &str) -> Result<Ast, Undecidable> {
        self.bump();
        let index = if name == "cbrt" {
            3
        } else {
            let Some(Tok::Num(text)) = self.peek() else {
                return Err(Undecidable::new("a root with no whole index"));
            };
            let index = text.parse().unwrap_or(0);
            self.bump();
            self.expect(&Tok::Comma, "a root with no argument")?;
            root_index(index)?
        };
        let argument = self.parse_expr()?;
        self.expect(&Tok::RParen, "a function call with no closing bracket")?;
        Ok(make_root(argument, index))
    }

    /// Turn an identifier into a function call, a constant, or a variable.
    fn parse_name(&mut self, name: &str) -> Result<Ast, Undecidable> {
        if let Some(base) = name.strip_prefix("log_") {
            return self.parse_based_log(base);
        }
        if self.is_function(name) {
            return self.parse_call(name);
        }
        if matches!(name, "Theta" | "Omega") && self.peek() == Some(&Tok::LParen) {
            // A growth rate such as `Theta(n^2)` is a call that the grammar keeps whole.
            return self.parse_call(name);
        }
        if is_differential(name) {
            return Ok(Ast::Var(name.to_string()));
        }
        if let Some(plain) = inverse_reciprocal(name) {
            return self.parse_inverse_reciprocal(plain);
        }
        let stem = name.trim_end_matches('\'');
        if stem.len() < name.len() && stem.chars().count() == 1 {
            // `x'` is the complement of `x`, or the derivative mark of `x`: one name.
            return Ok(Ast::Var(name.to_string()));
        }
        if name.contains('_') {
            // A subscripted name such as `a_n` or `u_n-1` is one variable.
            return Ok(Ast::Var(name.to_string()));
        }
        if name == "pi" {
            return Ok(Ast::Const(Const::Pi));
        }
        // Only the lower case `e` is the constant; `E` is a variable name.
        if name == "e" {
            return Ok(Ast::Const(Const::E));
        }
        if GREEK_VARIABLES.contains(&name) {
            return Ok(Ast::Var(name.to_string()));
        }
        if name.chars().count() == 1 {
            return Ok(Ast::Var(name.to_string()));
        }
        if super::segment::is_item_variable(name) {
            // The item names this variable (`rho`, `nT`), so it is one variable.
            return Ok(Ast::Var(name.to_string()));
        }
        Err(Undecidable::new(
            "a name that is not a function or variable",
        ))
    }

    /// Parse `arcsec(x)` as `acos(1/x)` and `arccsc(x)` as `asin(1/x)`.
    ///
    /// The caller took the name. The two identities hold on the principal
    /// branches of the inverse functions, for every `x` where `arcsec` or `arccsc` exists.
    fn parse_inverse_reciprocal(&mut self, plain: &str) -> Result<Ast, Undecidable> {
        match self.parse_call(plain)? {
            Ast::Func(name, args) if args.len() == 1 => {
                let reciprocal = args
                    .into_iter()
                    .next()
                    .map(|argument| make_quotient(Ast::Integer(BigInt::from(1)), argument))
                    .transpose()?
                    .unwrap_or(Ast::Integer(BigInt::from(1)));
                Ok(Ast::Func(name, vec![reciprocal]))
            }
            _ => Err(Undecidable::new(
                "a function call with the wrong count of arguments",
            )),
        }
    }

    /// Parse `log_b(x)`, `log_2 x`, or `log2(x)` as `log(x, base)`.
    fn parse_based_log(&mut self, base: &str) -> Result<Ast, Undecidable> {
        let base_ast = if base.starts_with(|c: char| c.is_ascii_digit()) {
            Ast::Integer(
                base.parse::<BigInt>()
                    .map_err(|_| Undecidable::new("a logarithm base outside the grammar"))?,
            )
        } else if base == "e" {
            Ast::Const(Const::E)
        } else if base.chars().count() == 1 {
            Ast::Var(base.to_string())
        } else {
            return Err(Undecidable::new("a logarithm base outside the grammar"));
        };
        let call = self.parse_call("log")?;
        match call {
            Ast::Func(name, mut args) if name == "log" && args.len() == 1 => {
                args.push(base_ast);
                Ok(Ast::Func(name, args))
            }
            _ => Err(Undecidable::new(
                "a logarithm with a base and a second base",
            )),
        }
    }

    /// The exponent of a power with a variable: a bracketed expression or one atom.
    fn free_exponent(&mut self) -> Result<Ast, Undecidable> {
        if self.eat(&Tok::LParen) {
            let inner = self.parse_expr()?;
            self.expect(&Tok::RParen, "an exponent with no closing bracket")?;
            Ok(inner)
        } else {
            let negative = self.eat(&Tok::Minus);
            let atom = self.parse_atom()?;
            Ok(if negative {
                Ast::Neg(Box::new(atom))
            } else {
                atom
            })
        }
    }

    /// Parse the argument of a whitelisted function, with or without brackets.
    pub(super) fn parse_call(&mut self, name: &str) -> Result<Ast, Undecidable> {
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
        // `sin^-1(x)` is the inverse sine, the way a calculator key writes it.
        if power == Some(Exponent::Whole(-1))
            && let Some(inverse) = ["asin", "acos", "atan"]
                .into_iter()
                .zip(["sin", "cos", "tan"])
                .find_map(|(inverse, plain)| (plain == name).then_some(inverse))
        {
            return Ok(make_call(inverse, vec![argument]));
        }
        let call = make_call(name, vec![argument]);
        Ok(match power {
            Some(exponent) => raise(call, exponent),
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
                parser.check_implicit_factor(&factors)?;
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
            Some(Tok::Sqrt(_) | Tok::NthRoot(..) | Tok::Root | Tok::IndexedRoot(_)) => true,
            _ => false,
        }
    }

    /// Parse `( … )`: a group, an ordered tuple, or the open end of an interval.
    ///
    /// The caller saw the opening bracket at the cursor.
    fn parse_paren_group(&mut self) -> Result<Ast, Undecidable> {
        self.bump();
        let mut items = self.parse_comma_list()?;
        if self.eat(&Tok::RBrack) {
            return make_interval(items, false, true);
        }
        self.expect(&Tok::RParen, "a group with no closing bracket")?;
        if items.len() == 1 {
            return Ok(items.swap_remove(0));
        }
        Ok(Ast::Tuple(items))
    }

    /// Parse `[ … ]`: an ordered list, or the closed end of an interval.
    ///
    /// The caller saw the opening bracket at the cursor.
    fn parse_bracket_group(&mut self) -> Result<Ast, Undecidable> {
        self.bump();
        let items = self.parse_comma_list()?;
        if self.eat(&Tok::RParen) {
            return make_interval(items, true, false);
        }
        self.expect(&Tok::RBrack, "a list with no closing bracket")?;
        Ok(Ast::List(items))
    }

    /// Parse a comma-separated body up to `close`.
    fn parse_items(&mut self, close: &Tok, reason: &'static str) -> Result<Vec<Ast>, Undecidable> {
        if self.eat(close) {
            return Ok(Vec::new());
        }
        let items = self.parse_comma_list()?;
        self.expect(close, reason)?;
        Ok(items)
    }

    /// Parse one expression, and every further expression a comma introduces.
    fn parse_comma_list(&mut self) -> Result<Vec<Ast>, Undecidable> {
        let mut items = vec![self.parse_expr()?];
        while self.eat(&Tok::Comma) {
            items.push(self.parse_expr()?);
        }
        Ok(items)
    }
}
