//! Whether a hint rung states an answer (Hard Rule 3), read as whole tokens.
//!
//! A plain substring test refused nearly every ladder by accident: the answer `1`
//! stands inside `g^{-1}`, `2` inside `x^2`, `0` inside `x_0`, and `a` inside any
//! word. This reader splits the rung into number, word and symbol tokens first,
//! then asks whether the answer stands as a whole token or a whole token run.
//!
//! The rules, by kind of answer:
//!
//! - A number matches a standalone number token of the same value, or a number
//!   word of that value. An exponent, a subscript, a digit glued to a letter and
//!   a part of a longer number never match. `1 / 2` matches as one fraction.
//! - A one-letter answer matches only inside math mode, as a symbol the rung sets
//!   equal to the result (`= a`).
//! - A word or a phrase matches as a whole-word run, ignoring case and articles.
//! - Any other expression matches as the same token run. One longer than
//!   [`LONG_EXPRESSION`] characters also matches as the same canonical form when
//!   both sides parse.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use crate::answer::canon::canon;
use crate::answer::parse::parse;

/// An expression answer longer than this also matches by canonical form.
const LONG_EXPRESSION: usize = 12;

/// The words the reader counts as numbers.
const NUMBER_WORDS: [(&str, i64); 28] = [
    ("zero", 0),
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// A number with its value. `plain` is false for an exponent, a subscript
    /// and a digit glued to a letter.
    Num {
        value: BigRational,
        plain: bool,
    },
    Word(String),
    Sym(char),
}

#[derive(Debug, Clone)]
struct Tok {
    kind: Kind,
    math: bool,
}

/// Whether the rung states the answer.
pub(crate) fn names_answer(rung: &str, answer: &str) -> bool {
    let answer = answer.trim();
    if answer.is_empty() {
        return false;
    }
    let toks = tokenize(rung);
    if let Some(value) = parse_rational(answer) {
        return names_number(&toks, &value, answer.contains('/'));
    }
    let mut letters = answer.chars();
    if let (Some(only), None) = (letters.next(), letters.next())
        && only.is_alphabetic()
    {
        return names_letter(&toks, only);
    }
    let wanted = without_articles(
        tokenize(answer)
            .into_iter()
            .map(|tok| tok.kind)
            .collect::<Vec<_>>(),
    );
    let have = without_articles(toks.iter().map(|tok| tok.kind.clone()).collect());
    if has_run(&have, &wanted) {
        return true;
    }
    let is_phrase = answer
        .chars()
        .all(|c| c.is_alphabetic() || c == ' ' || c == '-' || c == '\'');
    answer.chars().count() > LONG_EXPRESSION && !is_phrase && names_canonical(rung, answer)
}

/// `fraction` is true for an answer written as `n/d`: only a written fraction
/// of that value names it, because `1` does not state `4/4`.
fn names_number(toks: &[Tok], wanted: &BigRational, fraction: bool) -> bool {
    for (index, tok) in toks.iter().enumerate() {
        let before = index.checked_sub(1).and_then(|i| toks.get(i));
        let after = toks.get(index + 1);
        let slash_before = before.is_some_and(|t| t.kind == Kind::Sym('/'));
        let slash_after = after.is_some_and(|t| t.kind == Kind::Sym('/'));
        match &tok.kind {
            Kind::Num { value, plain } => {
                if !*plain {
                    continue;
                }
                if slash_after {
                    // numerator of `n / d`: the fraction as a whole is the value.
                    if let Some(Tok {
                        kind: Kind::Num { value: den, .. },
                        ..
                    }) = toks.get(index + 2)
                        && !den.is_zero()
                        && &(value / den) == wanted
                    {
                        return true;
                    }
                    continue;
                }
                if slash_before || fraction {
                    continue;
                }
                if value == wanted {
                    return true;
                }
            }
            Kind::Word(word) => {
                if !fraction
                    && !tok.math
                    && !slash_before
                    && !slash_after
                    && number_word_stands(before, after)
                    && declarative_clause(toks, index)
                    && number_word(word).is_some_and(|v| &v == wanted)
                {
                    return true;
                }
            }
            Kind::Sym(_) => {}
        }
    }
    false
}

/// Whether a number word follows a word that equates and closes its clause.
/// `a power of ten,` and `two factor-count classes` count or measure a thing.
fn number_word_stands(before: Option<&Tok>, after: Option<&Tok>) -> bool {
    let closes = after.is_none_or(|t| matches!(t.kind, Kind::Sym(_)));
    let equates = before.is_some_and(|t| match &t.kind {
        Kind::Word(w) => matches!(
            w.to_lowercase().as_str(),
            "is" | "are" | "be" | "equals" | "equal" | "answer" | "result"
        ),
        Kind::Sym(c) => *c == '=',
        Kind::Num { .. } => false,
    });
    equates && closes
}

/// Whether the token at `index` sits in a declarative result clause: its
/// sentence is not a question and no `if`, `when`, `whether`, `which`, `where`
/// or `unless` opens the clause before it.
fn declarative_clause(toks: &[Tok], index: usize) -> bool {
    let is_stop = |t: &Tok| matches!(t.kind, Kind::Sym('.' | '?' | '!' | ';'));
    let head = toks.get(..index).unwrap_or(&[]);
    let tail = toks.get(index..).unwrap_or(&[]);
    let sentence_start = head.iter().rposition(is_stop).map_or(0, |at| at + 1);
    let question = tail
        .iter()
        .position(is_stop)
        .and_then(|at| tail.get(at))
        .is_some_and(|t| t.kind == Kind::Sym('?'));
    let clause = head.get(sentence_start..).unwrap_or(&[]);
    let clause_start = clause
        .iter()
        .rposition(|t| t.kind == Kind::Sym(','))
        .map_or(0, |at| at + 1);
    let introduced = clause.get(clause_start..).unwrap_or(&[]).iter().any(|t| {
        matches!(&t.kind, Kind::Word(w) if matches!(
            w.to_lowercase().as_str(),
            "if" | "when" | "whether" | "which" | "where" | "unless"
        ))
    });
    !question && !introduced
}

fn number_word(word: &str) -> Option<BigRational> {
    let lower = word.to_lowercase();
    NUMBER_WORDS
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, value)| BigRational::from_integer(BigInt::from(*value)))
}

fn names_letter(toks: &[Tok], letter: char) -> bool {
    toks.iter().enumerate().any(|(index, tok)| {
        let Kind::Word(word) = &tok.kind else {
            return false;
        };
        let mut chars = word.chars();
        if !tok.math || chars.next() != Some(letter) || chars.next().is_some() {
            return false;
        }
        let equated = index
            .checked_sub(1)
            .and_then(|i| toks.get(i))
            .is_some_and(|t| t.math && t.kind == Kind::Sym('='));
        let continues = toks.get(index + 1).is_some_and(|t| match &t.kind {
            Kind::Num { .. } | Kind::Word(_) => t.math,
            Kind::Sym(c) => matches!(c, '^' | '_' | '+' | '-' | '*' | '/' | '(' | '\u{221a}'),
        });
        equated && !continues
    })
}

fn is_article(kind: &Kind) -> bool {
    matches!(kind, Kind::Word(w) if matches!(w.to_lowercase().as_str(), "a" | "an" | "the"))
}

fn without_articles(kinds: Vec<Kind>) -> Vec<Kind> {
    kinds.into_iter().filter(|kind| !is_article(kind)).collect()
}

fn same_kind(left: &Kind, right: &Kind) -> bool {
    match (left, right) {
        (Kind::Num { value: a, .. }, Kind::Num { value: b, .. }) => a == b,
        (Kind::Word(a), Kind::Word(b)) => a.to_lowercase() == b.to_lowercase(),
        (Kind::Sym(a), Kind::Sym(b)) => a == b,
        _ => false,
    }
}

fn has_run(have: &[Kind], wanted: &[Kind]) -> bool {
    !wanted.is_empty()
        && have.windows(wanted.len()).any(|window| {
            window
                .iter()
                .zip(wanted)
                .all(|(left, right)| same_kind(left, right))
        })
}

/// Whether a math piece of the rung has the answer's canonical form.
fn names_canonical(rung: &str, answer: &str) -> bool {
    let Some(want) = parse(&latex_to_plain(answer))
        .ok()
        .and_then(|ast| canon(&ast).ok())
    else {
        return false;
    };
    math_spans(rung).iter().any(|span| {
        span.split('=').any(|piece| {
            let piece = latex_to_plain(piece);
            let piece = piece.trim().trim_end_matches(['.', ',', ';']);
            parse(piece)
                .ok()
                .and_then(|ast| canon(&ast).ok())
                .is_some_and(|got| got == want)
        })
    })
}

/// The text between `$` marks.
fn math_spans(rung: &str) -> Vec<String> {
    rung.split('$')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, span)| span.to_owned())
        .collect()
}

/// A LaTeX span as plain text: fractions as `(a)/(b)`, commands dropped, braces
/// as parentheses.
fn latex_to_plain(source: &str) -> String {
    let source = rewrite_fractions(source, "(", ")/(", ")");
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                while chars.peek().is_some_and(char::is_ascii_alphabetic) {
                    chars.next();
                }
            }
            '{' => out.push('('),
            '}' => out.push(')'),
            other => out.push(other),
        }
    }
    out
}

/// Replace every `\frac{a}{b}` with `{open}a{middle}b{close}`.
fn rewrite_fractions(source: &str, open: &str, middle: &str, close: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    loop {
        let found = ["\\dfrac", "\\tfrac", "\\frac"]
            .iter()
            .filter_map(|name| rest.find(name).map(|at| (at, name.len())))
            .min();
        let Some((at, width)) = found else {
            out.push_str(rest);
            return out;
        };
        out.push_str(rest.get(..at).unwrap_or(""));
        let after = rest.get(at + width..).unwrap_or("");
        let Some((top, after_top)) = brace_group(after) else {
            rest = after;
            continue;
        };
        let Some((bottom, after_bottom)) = brace_group(after_top) else {
            rest = after;
            continue;
        };
        out.push_str(open);
        out.push_str(&rewrite_fractions(top, open, middle, close));
        out.push_str(middle);
        out.push_str(&rewrite_fractions(bottom, open, middle, close));
        out.push_str(close);
        rest = after_bottom;
    }
}

/// The inside of one leading `{…}` group and the text after it.
fn brace_group(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_start();
    let inner = trimmed.strip_prefix('{')?;
    let mut depth = 1_usize;
    for (at, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((inner.get(..at)?, inner.get(at + 1..)?));
                }
            }
            _ => {}
        }
    }
    None
}

/// A plain number: sign, digits with optional thousands commas, a decimal part,
/// or one `n/d` fraction.
fn parse_rational(text: &str) -> Option<BigRational> {
    let text: String = text.trim().replace(',', "");
    if let Some((top, bottom)) = text.split_once('/') {
        let bottom = parse_decimal(bottom.trim())?;
        if bottom.is_zero() {
            return None;
        }
        return Some(parse_decimal(top.trim())? / bottom);
    }
    parse_decimal(&text)
}

fn parse_decimal(text: &str) -> Option<BigRational> {
    let (negative, digits) = match text.strip_prefix(['-', '\u{2212}']) {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let numer: BigInt = format!("{whole}{fraction}").parse().ok()?;
    let denom = BigInt::from(10).pow(u32::try_from(fraction.len()).ok()?);
    let value = BigRational::new(numer, denom);
    Some(if negative { -value } else { value })
}

fn tokenize(rung: &str) -> Vec<Tok> {
    let rung = rewrite_fractions(rung, "{", " / ", "}");
    let chars: Vec<char> = rung.chars().collect();
    let mut toks: Vec<Tok> = Vec::new();
    let mut math = false;
    // One flag per open brace group: is the group an exponent or a subscript.
    let mut groups: Vec<bool> = Vec::new();
    let mut pending_script = false;
    let mut at = 0_usize;
    while let Some(&c) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        let in_script = pending_script || groups.iter().any(|g| *g);
        if c.is_whitespace() {
            at += 1;
            continue;
        }
        if c == '$' {
            math = !math;
            at += 1;
            continue;
        }
        if c == '\\' {
            at += 1;
            let mut name = String::new();
            while let Some(&l) = chars.get(at)
                && l.is_ascii_alphabetic()
            {
                name.push(l);
                at += 1;
            }
            if name.is_empty() {
                match chars.get(at) {
                    Some('(' | '[') => math = true,
                    Some(')' | ']') => math = false,
                    _ => {}
                }
                at += 1;
            } else if name == "sqrt" {
                toks.push(Tok {
                    kind: Kind::Sym('\u{221a}'),
                    math,
                });
                pending_script = true;
            }
            continue;
        }
        if c == '{' {
            groups.push(pending_script || groups.iter().any(|g| *g));
            pending_script = false;
            at += 1;
            continue;
        }
        if c == '}' {
            groups.pop();
            at += 1;
            continue;
        }
        let glued_left = at
            .checked_sub(1)
            .and_then(|i| chars.get(i))
            .is_some_and(|p| p.is_alphabetic());
        let signed = matches!(c, '-' | '\u{2212}')
            && next.is_some_and(|n| n.is_ascii_digit())
            && !at
                .checked_sub(1)
                .and_then(|i| chars.get(i))
                .is_some_and(|p| p.is_alphanumeric() || matches!(p, ')' | ']'));
        if c.is_ascii_digit() || signed {
            let start = at;
            at += usize::from(signed);
            at = scan_digits(&chars, at);
            let text: String = chars.get(start..at).unwrap_or(&[]).iter().collect();
            let letters_after = chars
                .get(at..)
                .unwrap_or(&[])
                .iter()
                .take_while(|l| l.is_alphabetic())
                .count();
            let glued_right = letters_after > 0 && (!math || letters_after == 1);
            if let Some(value) = parse_rational(&text) {
                toks.push(Tok {
                    kind: Kind::Num {
                        value,
                        plain: !in_script && !(glued_left && !signed) && !glued_right,
                    },
                    math,
                });
            }
            pending_script = false;
            continue;
        }
        if c.is_alphabetic() {
            let start = at;
            while chars.get(at).is_some_and(|l| l.is_alphabetic()) {
                at += 1;
            }
            let word: String = chars.get(start..at).unwrap_or(&[]).iter().collect();
            toks.push(Tok {
                kind: Kind::Word(word),
                math,
            });
            pending_script = false;
            continue;
        }
        if matches!(c, '^' | '_') {
            pending_script = true;
        } else if !(c == '-' && pending_script) {
            pending_script = false;
        }
        toks.push(Tok {
            kind: Kind::Sym(c),
            math,
        });
        at += 1;
    }
    toks
}

/// The end of a number that starts at `at`: digits, thousands commas and one
/// decimal part. A point or a comma with no digit after it ends the number.
fn scan_digits(chars: &[char], mut at: usize) -> usize {
    let digit_at = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    loop {
        let joiner = matches!(chars.get(at), Some('.' | ',')) && digit_at(at + 1);
        if digit_at(at) || joiner {
            at += 1;
        } else {
            return at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::names_answer;

    fn refused(rung: &str, answer: &str) -> bool {
        names_answer(rung, answer)
    }

    #[test]
    fn inverse_exponent_is_not_the_answer_one() {
        assert!(!refused("Apply $g^{-1}$ to both sides.", "1"));
        assert!(!refused("Apply $g^{-1}$ to both sides.", "-1"));
    }

    #[test]
    fn exponent_is_not_the_answer_two() {
        assert!(!refused("Square the base: $x^2$.", "2"));
        assert!(!refused("Look at $x^{n+2}$.", "2"));
        assert!(!refused("Look at $x_2$ and $y_{12}$.", "2"));
    }

    #[test]
    fn subscript_is_not_the_answer_zero() {
        assert!(!refused("Start from $x_0$.", "0"));
        assert!(!refused("Start from $x_{0}$.", "0"));
    }

    #[test]
    fn digit_inside_a_longer_number_is_not_the_answer() {
        assert!(!refused("Read 12 and 2023.", "2"));
        assert!(!refused("Read 0.5.", "0"));
    }

    #[test]
    fn letter_in_a_word_is_not_the_answer_a() {
        assert!(!refused("Name a variable and add a bar.", "a"));
        assert!(!refused("Take $a + b$ and $a$ times $b$.", "a"));
        assert!(!refused("Let $n$ be the count.", "n"));
    }

    #[test]
    fn phrase_needs_the_whole_word_run() {
        assert!(!refused(
            "No solutions exist unless the sign flips.",
            "no solution"
        ));
        assert!(refused("There is no solution here.", "no solution"));
        assert!(refused("There is NO solution here.", "no solution"));
        assert!(refused("There is no solution here.", "No the solution"));
    }

    #[test]
    fn sentence_ending_number_is_refused() {
        assert!(refused("So the answer is 12.", "12"));
        assert!(refused("It costs $12 in all.", "12"));
    }

    #[test]
    fn equated_value_is_refused() {
        assert!(refused("which gives $x = 3$", "3"));
        assert!(refused("so \\(x = 3\\)", "3"));
    }

    #[test]
    fn number_word_counts_as_the_number() {
        assert!(refused("the order is two", "2"));
        assert!(refused("The total is twelve", "12"));
        assert!(!refused("the order is three", "2"));
        assert!(!refused("Use a power of ten, then add.", "10"));
        assert!(!refused(
            "Which digits are zero, and what do their places contribute to the sum?",
            "0"
        ));
        assert!(!refused(
            "When the digit you need to borrow from is zero, where do you go next?",
            "0"
        ));
        assert!(!refused(
            "If a part is zero, what does it contribute to the total?",
            "0"
        ));
        assert!(!refused(
            "If the remainder is zero, what does that say about factor and multiple?",
            "0"
        ));
        assert!(!refused(
            "If the only factor the two numbers share is one, what is the GCF?",
            "1"
        ));
        assert!(!refused(
            "If the only factor the numbers share is one, how does their product relate to the LCM?",
            "1"
        ));
        assert!(refused("The order is two.", "2"));
        assert!(!refused("Is two a factor of the number?", "2"));
        assert!(!refused("Is it one of the two factor-count classes?", "2"));
    }

    #[test]
    fn fraction_matches_as_a_whole() {
        assert!(refused("so $\\frac{1}{2}$ is left", "1/2"));
        assert!(refused("so $\\frac{2}{4}$ is left", "1/2"));
        assert!(refused("so $\\frac{1}{2}$ is left", "0.5"));
        assert!(!refused("the whole number 1 is left", "4/4"));
        assert!(!refused("so $\\frac{1}{2}$ is left", "2"));
        assert!(!refused("so $\\frac{1}{2}$ is left", "1"));
    }

    #[test]
    fn one_letter_answer_needs_math_equation() {
        assert!(refused("so $y = a$", "a"));
        assert!(refused("so $ y = a. $", "a"));
        assert!(!refused("so y = a", "a"));
        assert!(!refused("so $y = a + 1$", "a"));
        assert!(!refused("so $a = y$", "a"));
    }

    #[test]
    fn short_expression_matches_as_a_token_run() {
        assert!(refused("so $x^2 + 1$ appears", "x^2 + 1"));
        assert!(!refused("so $x^3 + 1$ appears", "x^2 + 1"));
    }

    #[test]
    fn long_expression_matches_by_canonical_form() {
        assert!(refused("so $x = \\frac{3}{6}$ here", "1/3 + 1/6 + 0 + 0"));
        assert!(refused("so $x = 2/4$ here", "1/3 + 1/6 + 0 + 0"));
        assert!(!refused("so $x = 3/4$ here", "1/3 + 1/6 + 0 + 0"));
    }

    #[test]
    fn empty_answer_names_nothing() {
        assert!(!refused("anything", ""));
    }
}
