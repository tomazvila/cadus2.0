//! The unary, power, and atom productions, and the bracketed groups.

use num_bigint::BigInt;

use super::build::{collapse, letter_run, make_call, make_interval, make_quotient, parse_number};
use super::{GREEK_VARIABLES, MAX_EXPONENT, Parser};
use crate::answer::Undecidable;
use crate::answer::ast::{Ast, Const};
use crate::answer::lexer::Tok;

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
        let mut items = self.parse_comma_list()?;
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
