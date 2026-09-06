//! Part 7 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

/// Walk one canonical form and collect its radical atoms.
pub fn collect_radical_atoms(canon: &Canon, out: &mut BTreeSet<String>) {
    match canon {
        Canon::Rational(_) => {}
        Canon::Radical(terms) => {
            for basis in terms.keys() {
                if !basis.radicand.is_one() {
                    out.insert(format!("sqrt({})", basis.radicand));
                }
            }
        }
        Canon::Poly(poly) => collect_poly_radicals(poly, out),
        Canon::Value { num, den } => {
            collect_poly_radicals(num, out);
            collect_poly_radicals(den, out);
        }
        Canon::Func(_, args) | Canon::Tuple(args) | Canon::List(args) => {
            for arg in args {
                collect_radical_atoms(arg, out);
            }
        }
        Canon::Set(members) => {
            for member in members {
                collect_radical_atoms(member, out);
            }
        }
        Canon::Interval { lo, hi, .. } => {
            for end in [lo, hi].into_iter().flatten() {
                collect_radical_atoms(end, out);
            }
        }
        Canon::Assign { value, .. } => collect_radical_atoms(value, out),
    }
}

/// Collect the radical atoms of one polynomial.
pub fn collect_poly_radicals(poly: &cadus_core::answer::Poly, out: &mut BTreeSet<String>) {
    for (atom, exponent) in poly.keys().flatten() {
        match atom {
            Atom::Sqrt(radicand) => {
                out.insert(format!("sqrt({radicand})**{exponent}"));
            }
            Atom::Call(name, args) if name == "sqrt" => {
                out.insert(format!("sqrt({args:?})**{exponent}"));
                for arg in args {
                    collect_radical_atoms(arg, out);
                }
            }
            Atom::Call(_, args) => {
                for arg in args {
                    collect_radical_atoms(arg, out);
                }
            }
            Atom::Exp(inner) => collect_radical_atoms(inner, out),
            Atom::Root(base, index) => {
                out.insert(format!("root({base:?}, {index})**{exponent}"));
                collect_radical_atoms(base, out);
            }
            Atom::Pi | Atom::E | Atom::Var(_) => {}
        }
    }
}

/// The denominator of one canonical form, as text. Empty means "no denominator".
///
/// A [`Canon::Value`] carries its denominator in the `den` field. A
/// [`Canon::Poly`] carries a MONOMIAL denominator as the negative exponents of
/// its atoms, because FIXM2h moves a monomial divisor into the numerator.
pub fn denominator_key(canon: &Canon) -> String {
    match canon {
        Canon::Value { den, .. } => format!("{den:?}"),
        Canon::Poly(poly) => {
            let mut divisors: BTreeSet<String> = BTreeSet::new();
            for (atom, exponent) in poly.keys().flatten() {
                if *exponent < 0 {
                    divisors.insert(format!("{atom:?}**{}", -exponent));
                }
            }
            if divisors.is_empty() {
                String::new()
            } else {
                divisors.into_iter().collect::<Vec<String>>().join("*")
            }
        }
        _ => String::new(),
    }
}

/// The two canonical forms of one pair, when the checker decides both.
pub fn both_canonical_forms(pair: &Pair) -> Option<(Canon, Canon)> {
    let expected = canonical_form(&pair.expected).ok()?;
    let learner = canonical_form(&pair.learner).ok()?;
    Some((expected, learner))
}

/// Whether 2.0 refuses the pair because it runs no polynomial GCD (FIXM2h).
///
/// The predicate is SPECIFIC, and it names two facts that must both hold:
///
/// 1. `scripts/oracle/rewrite_1_0.py` recorded `cancel(expected - learner) == 0`,
///    so the two answers are one value and the difference needs a polynomial GCD.
/// 2. The two canonical forms differ IN A DENOMINATOR.
///
/// A pair that misses either fact keeps no reason. It stays in class 3, and it
/// fails the parity assertion as a 2.0 bug (R5). No catch-all sits here.
pub fn no_polynomial_gcd(pair: &Pair) -> bool {
    let key = (
        pair.expected.clone(),
        pair.learner.clone(),
        pair.kind.as_str().to_string(),
    );
    let Some((cancel_zero, _)) = rewrite_evidence().get(&key).copied() else {
        return false;
    };
    if !cancel_zero {
        return false;
    }
    let Some((expected, learner)) = both_canonical_forms(pair) else {
        return false;
    };
    let left = denominator_key(&expected);
    let right = denominator_key(&learner);
    left != right && !(left.is_empty() && right.is_empty())
}

/// Whether 2.0 refuses the pair because it rationalizes no radical (FIXM2h).
///
/// The predicate is SPECIFIC, and it names two facts that must both hold:
///
/// 1. `scripts/oracle/rewrite_1_0.py` recorded `radsimp(expected - learner) == 0`,
///    so the two answers are one value under the radical laws.
/// 2. The two canonical forms differ IN A RADICAL ATOM.
///
/// `sqrt(x)*sqrt(x)` is the shape: 2.0 keeps two `sqrt(x)` atoms and never folds
/// them into `x`, because the fold holds for a non-negative `x` only.
pub fn no_radical_rationalization(pair: &Pair) -> bool {
    let key = (
        pair.expected.clone(),
        pair.learner.clone(),
        pair.kind.as_str().to_string(),
    );
    let Some((_, radsimp_zero)) = rewrite_evidence().get(&key).copied() else {
        return false;
    };
    if !radsimp_zero {
        return false;
    }
    let Some((expected, learner)) = both_canonical_forms(pair) else {
        return false;
    };
    let left = radical_atoms(&expected);
    let right = radical_atoms(&learner);
    left != right && !(left.is_empty() && right.is_empty())
}

/// Name the documented reason a pair diverges, when one covers it.
///
/// The order is fixed, so one pair gets one reason. A pair that no predicate
/// covers stays in class 3 and fails the parity assertion (R5).
pub fn documented_reason(
    pair: &Pair,
    rust_correct: bool,
    oracle: OracleVerdict,
) -> Option<&'static str> {
    if pair.shape == "prose_or_words" {
        return Some("prose is not a value (V2)");
    }
    // 2.0 reads a learner decimal as the exact rounding of the authored value,
    // and it marks the FORM with the notation tag (ruling `D6-dec`). 1.0 had no
    // such rung: its float tolerance accepted a rounding inside 1e-6 with no
    // note, and refused every rounding outside it. The branch runs FIRST,
    // because the pair carries a 2.0 `correct` and the two branches below both
    // ask about a disagreement over `correct` alone.
    if rust_correct && the_learner_wrote_the_exact_rounding(pair) {
        return Some(if oracle.equivalent {
            "the exact rounding carries the notation tag (D6-dec)"
        } else {
            "an exact rounding 1.0 refused is correct (D6-dec)"
        });
    }
    if oracle.equivalent && !rust_correct {
        return narrowing_reason(pair);
    }
    if !oracle.equivalent && rust_correct {
        return refusal_reason(pair);
    }
    None
}

/// The reason 2.0 says no where 1.0 said yes, when one is documented.
fn narrowing_reason(pair: &Pair) -> Option<&'static str> {
    if the_1_0_float_rung_closes_the_gap(pair) {
        return Some("no float tolerance rung (D6)");
    }
    // The two narrowings of the canonical rational form, in a fixed order.
    // A radical shape takes the radical reason, and every other shape takes
    // the GCD reason, so one pair gets one reason.
    if no_radical_rationalization(pair) {
        return Some("no radical rationalization (V1 narrowing)");
    }
    if no_polynomial_gcd(pair) {
        return Some("no polynomial GCD (V1 narrowing)");
    }
    // No catch-all sits here. A 1.0 `simplify` result is not readable from
    // the two answer strings, so a pair that names a transcendental function
    // is NOT a transcendental identity by that fact alone: the five
    // `cos 2*x` pairs of M2 review 2, findings 10 and 14, are a parse
    // divergence and the old substring test hid them. An unexplained
    // divergence stays in class 3 and fails the parity assertion (R5).
    None
}

/// The reason 2.0 says yes where 1.0 said no: a 1.0 stage refused the answer.
///
/// The rules run in a fixed order, so one pair gets one reason.
fn refusal_reason(pair: &Pair) -> Option<&'static str> {
    refusal_rules()
        .into_iter()
        .find(|(rule, _)| rule(pair))
        .map(|(_, reason)| reason)
}

/// Whether `rule` holds on either answer of the pair.
fn either_side(pair: &Pair, rule: fn(&str) -> bool) -> bool {
    rule(&pair.expected) || rule(&pair.learner)
}

/// One refusal rule of 1.0, with the reason it names.
type RefusalRule = (fn(&Pair) -> bool, &'static str);

/// The refusal rules of 1.0, each with the reason it names, in order.
fn refusal_rules() -> [RefusalRule; 8] {
    [
        (
            |pair| either_side(pair, nineteen_zero_reads_a_power_tower),
            "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
        ),
        (
            |pair| either_side(pair, nineteen_zero_reads_a_python_literal),
            "the 1.0 tokenizer reads a Python number literal (spec 3.1)",
        ),
        (
            |pair| either_side(pair, a_unicode_radical_over_a_nested_group),
            "the 1.0 radical rewrite misses a nested group (spec 2.2)",
        ),
        (
            |pair| a_chained_inequality(&pair.expected),
            "a chained inequality raises inside 1.0 (spec 7.7)",
        ),
        (
            |pair| either_side(pair, nineteen_zero_leaves_a_brace_group),
            "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
        ),
        (
            one_side_alone_names_a_bare_e,
            "the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)",
        ),
        (
            a_spaced_times_x,
            "2.0 reads a spaced `x` as the times sign (review 1, finding 18)",
        ),
        (
            |pair| either_side(pair, a_bracket_free_argument_meets_a_function),
            "the juxtaposed argument stops at a function name (review 3, finding 5)",
        ),
    ]
}

/// The names the 2.0 grammar reads as functions (`answer::parse`, `FUNCTIONS`).
///
/// The list is a literal copy, so a change in the grammar cannot quietly change
/// the reason a pair leaves class 3.
pub const FUNCTION_NAMES: [&str; 17] = [
    "sqrt", "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "sinh", "cosh",
    "tanh", "exp", "ln", "log", "abs",
];

/// Whether one side writes a times sign as the spaced letter `x` (or `X`).
///
/// 2.0 reads a spaced `x` between two values as multiplication (M2 review 1,
/// finding #18), and 1.0 hands the letter to SymPy as a free symbol. The
/// predicate is specific: the 1.0 source of the side must name the bare letter,
/// AND the 2.0 value of that same side must carry no variable of that name. A
/// side that really does hold the variable `x` keeps no reason here.
pub fn a_spaced_times_x(pair: &Pair) -> bool {
    [&pair.expected, &pair.learner]
        .into_iter()
        .any(|side| names_a_bare_word(&one_zero_source(side), "x") && !holds_the_variable_x(side))
}

/// Whether the 2.0 value of `answer` carries the variable `x`.
pub fn holds_the_variable_x(answer: &str) -> bool {
    let Ok(canon) = canonical_form(answer) else {
        return false;
    };
    let mut names = BTreeSet::new();
    collect_variable_names(&canon, &mut names);
    names.contains("x")
}

/// Collect the variable names of one canonical form.
pub fn collect_variable_names(canon: &Canon, out: &mut BTreeSet<String>) {
    match canon {
        Canon::Rational(_) | Canon::Radical(_) => {}
        Canon::Poly(poly) => collect_poly_variables(poly, out),
        Canon::Value { num, den } => {
            collect_poly_variables(num, out);
            collect_poly_variables(den, out);
        }
        Canon::Func(_, args) | Canon::Tuple(args) | Canon::List(args) => {
            for arg in args {
                collect_variable_names(arg, out);
            }
        }
        Canon::Set(members) => {
            for member in members {
                collect_variable_names(member, out);
            }
        }
        Canon::Interval { var, lo, hi, .. } => {
            if let Some(name) = var {
                out.insert(name.clone());
            }
            for end in [lo, hi].into_iter().flatten() {
                collect_variable_names(end, out);
            }
        }
        Canon::Assign { value, .. } => collect_variable_names(value, out),
    }
}

/// Collect the variable names of one polynomial.
pub fn collect_poly_variables(poly: &cadus_core::answer::Poly, out: &mut BTreeSet<String>) {
    for (atom, _) in poly.keys().flatten() {
        match atom {
            Atom::Var(name) => {
                out.insert(name.clone());
            }
            Atom::Call(_, args) => {
                for arg in args {
                    collect_variable_names(arg, out);
                }
            }
            Atom::Exp(inner) | Atom::Root(inner, _) => collect_variable_names(inner, out),
            Atom::Sqrt(_) | Atom::Pi | Atom::E => {}
        }
    }
}

/// Whether `text` holds the whole word `word`.
pub fn names_a_bare_word(text: &str, word: &str) -> bool {
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            run.push(ch);
            continue;
        }
        if run.eq_ignore_ascii_case(word) {
            return true;
        }
        run.clear();
    }
    run.eq_ignore_ascii_case(word)
}

/// Whether `text` writes a bracket-free function argument that a second function
/// name follows.
///
/// `sec x tan x` is the shape. 2.0 stops the argument at the second name and
/// reads `sec(x)*tan(x)`; SymPy `implicit_multiplication_application` swallows
/// the name and reads `sec(x*tan(x))` (M2 review 3, finding #5, and the ruling of
/// that round). The predicate names the construct, and it fires on no other:
/// the first name must carry NO bracket, and a second function name must follow
/// it before any bracket or operator.
pub fn a_bracket_free_argument_meets_a_function(text: &str) -> bool {
    let words = word_runs(text);
    for (index, (word, follows_open)) in words.iter().enumerate() {
        if !FUNCTION_NAMES.contains(&word.as_str()) || *follows_open {
            continue;
        }
        // The argument runs on until the next function name, so the second name
        // is the first one that follows, at any distance.
        let follows = words
            .get(index + 1..)
            .unwrap_or_default()
            .iter()
            .any(|(later, _)| FUNCTION_NAMES.contains(&later.as_str()));
        if follows {
            return true;
        }
    }
    false
}

/// The word runs of `text`, each with a flag for a `(` that follows it.
pub fn word_runs(text: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars.get(index).copied().unwrap_or(' ');
        if !ch.is_ascii_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while matches!(chars.get(index), Some(c) if c.is_ascii_alphanumeric() || *c == '_') {
            index += 1;
        }
        let word: String = chars.get(start..index).unwrap_or_default().iter().collect();
        let mut scan = index;
        while matches!(chars.get(scan), Some(c) if c.is_whitespace()) {
            scan += 1;
        }
        out.push((word, chars.get(scan) == Some(&'(')));
    }
    out
}

/// Whether one side alone writes Euler's number as the bare name `e`.
///
/// `_safe_sympy_globals` runs `from sympy import *`, and that namespace holds
/// `E` and `exp` and no lowercase `e` (`sympy_check.py:170`). `parse_expr` then
/// reads `e` as a free symbol, so 1.0 grades `e**2` against `exp(2)` False. 2.0
/// reads `e` and `E` as one constant (`crates/core/tests/answer_divergence.rs`,
/// `e_is_eulers_number_on_both_sides`).
///
/// The predicate is specific: it fires only when ONE side carries the bare name.
/// Two sides that both carry it reach the same free symbol in 1.0, so 1.0 and
/// 2.0 agree on that pair and the divergence has another cause.
pub fn one_side_alone_names_a_bare_e(pair: &Pair) -> bool {
    names_a_bare_e(&one_zero_source(&pair.expected))
        != names_a_bare_e(&one_zero_source(&pair.learner))
}

/// Whether `source` names Euler's number as the bare token `e`.
///
/// A letter in front of the `e` makes it part of a longer name (`sec`, `exp`).
/// A digit in front does NOT: SymPy `implicit_multiplication` splits `3e**x`
/// into `3*e**x`, measured on 2026-08-27. A digit AFTER the `e` makes the whole
/// run a float literal, and `3e5` is the number 300000.
pub fn names_a_bare_e(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != 'e' {
            continue;
        }
        let before = index
            .checked_sub(1)
            .and_then(|i| chars.get(i))
            .copied()
            .unwrap_or(' ');
        let after = chars.get(index + 1).copied().unwrap_or(' ');
        if !before.is_alphabetic() && before != '_' && !after.is_alphanumeric() && after != '_' {
            return true;
        }
    }
    false
}

/// One classified pair.
pub struct Classified {
    pub class: Class,
    pub reason: Option<&'static str>,
    pub agreed: bool,
}
