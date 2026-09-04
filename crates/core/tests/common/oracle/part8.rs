//! Part 8 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

/// Classify one pair against its recorded 1.0 verdict.
pub fn classify(pair: &Pair, oracle: Option<OracleVerdict>) -> Classified {
    let Some(oracle) = oracle else {
        return Classified {
            class: Class::OracleSilent,
            reason: None,
            agreed: true,
        };
    };
    if pair.shape == "prose_or_words" {
        return Classified {
            class: Class::ProseExpected,
            reason: Some("prose is not a value (V2)"),
            agreed: true,
        };
    }
    let outcome = check(&pair.expected, &pair.learner, pair.kind);
    let verdict = match outcome {
        Outcome::Undecidable(_) => {
            return Classified {
                class: Class::OutsideGrammar,
                reason: None,
                agreed: true,
            };
        }
        Outcome::Decided(verdict) => verdict,
    };
    let agreed = verdict.correct == oracle.equivalent && verdict.notation == oracle.notation;
    if agreed {
        return Classified {
            class: Class::Comparable,
            reason: None,
            agreed: true,
        };
    }
    match documented_reason(pair, verdict.correct, oracle) {
        Some(reason) => Classified {
            class: Class::DocumentedDivergence,
            reason: Some(reason),
            agreed: true,
        },
        None => Classified {
            class: Class::Comparable,
            reason: None,
            agreed: false,
        },
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------
/// The counts the acceptance check quotes.
pub struct Report {
    pub pairs: usize,
    pub per_generator: BTreeMap<&'static str, usize>,
    pub per_intent: BTreeMap<&'static str, usize>,
    pub per_class: BTreeMap<&'static str, usize>,
    pub per_reason: BTreeMap<&'static str, usize>,
    /// One line per pair of each documented reason, for the review record.
    pub per_reason_pairs: BTreeMap<&'static str, Vec<String>>,
    pub comparable: usize,
    pub comparable_agreed: usize,
    pub disagreements: Vec<String>,
}

/// Run every generated pair against its recorded verdict and collect the counts.
pub fn build_report(pairs: &[Pair], verdicts: &BTreeMap<PairKey, Option<OracleVerdict>>) -> Report {
    let mut report = Report {
        pairs: pairs.len(),
        per_generator: BTreeMap::new(),
        per_intent: BTreeMap::new(),
        per_class: BTreeMap::new(),
        per_reason: BTreeMap::new(),
        per_reason_pairs: BTreeMap::new(),
        comparable: 0,
        comparable_agreed: 0,
        disagreements: Vec::new(),
    };
    for pair in pairs {
        *report.per_generator.entry(pair.generator).or_insert(0) += 1;
        *report.per_intent.entry(pair.intent.name()).or_insert(0) += 1;
        let key = (
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        );
        let oracle = verdicts.get(&key).copied().unwrap_or_else(|| {
            panic!(
                "the committed oracle file has no verdict for {:?} against {:?} on {}",
                pair.expected, pair.learner, pair.kind
            )
        });
        let classified = classify(pair, oracle);
        *report.per_class.entry(classified.class.name()).or_insert(0) += 1;
        if let Some(reason) = classified.reason {
            *report.per_reason.entry(reason).or_insert(0) += 1;
            report
                .per_reason_pairs
                .entry(reason)
                .or_default()
                .push(format!(
                    "{}: {:?} against {:?}",
                    pair.generator, pair.expected, pair.learner
                ));
        }
        if classified.class == Class::Comparable {
            report.comparable += 1;
            if classified.agreed {
                report.comparable_agreed += 1;
            } else {
                let outcome = check(&pair.expected, &pair.learner, pair.kind);
                report.disagreements.push(format!(
                    "{}: {:?} against {:?} on {} -> 2.0 {:?}, 1.0 {:?}",
                    pair.generator, pair.expected, pair.learner, pair.kind, outcome, oracle
                ));
            }
        }
    }
    report
}

/// Print the report the acceptance check quotes.
pub fn print_report(report: &Report) {
    println!("pairs: {}", report.pairs);
    println!("-- per generator --");
    for (name, count) in &report.per_generator {
        println!("{name}: {count}");
    }
    println!("-- per intent --");
    for (name, count) in &report.per_intent {
        println!("{name}: {count}");
    }
    println!("-- per class --");
    for (name, count) in &report.per_class {
        println!("{name}: {count}");
    }
    println!("-- per documented reason --");
    for (name, count) in &report.per_reason {
        println!("{name}: {count}");
    }
    // The two rewrite narrowings are the new divergences of FIXM2i, and the M2
    // plan quotes their pairs, so the report names every one of them. The float
    // rung joins them under ruling `D6-dec`: the rung held 240 pairs, the
    // rounding rule of FIX-D6 decides all but a few of them, and the report
    // names every pair that stays wrong.
    for reason in [
        "no polynomial GCD (V1 narrowing)",
        "no radical rationalization (V1 narrowing)",
        "no float tolerance rung (D6)",
    ] {
        for line in report.per_reason_pairs.get(reason).into_iter().flatten() {
            println!("[{reason}] {line}");
        }
    }
    let percent = if report.comparable == 0 {
        100.0
    } else {
        100.0 * report.comparable_agreed as f64 / report.comparable as f64
    };
    println!(
        "class 3 agreement: {}/{} = {percent:.4}%",
        report.comparable_agreed, report.comparable
    );
    for line in report.disagreements.iter().take(40) {
        println!("DISAGREE {line}");
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------
/// The literal size of the generated set.
pub const GENERATED_PAIRS: usize = 17_874;

/// The literal pair count of every generator, in name order.
pub const GENERATOR_COUNTS: [(&str, usize); 47] = [
    ("algebraic_refactor", 65),
    ("appended_junk", 1562),
    ("ascii_to_unicode", 70),
    ("caret_power", 0),
    ("case_flip", 735),
    ("coarse_decimal", 92),
    ("comma_space_removed", 197),
    ("comma_thousands", 44),
    ("decimal_to_fraction", 105),
    ("digit_transposition", 604),
    ("dollar_wrapped", 1298),
    ("dot_thousands", 44),
    ("equivalent_fraction", 165),
    ("explicit_multiplication", 351),
    ("figure_space_thousands", 44),
    ("fraction_to_decimal", 79),
    ("identity", 1562),
    ("implicit_multiplication", 270),
    ("internal_spaces", 1063),
    ("last_digit_bumped", 1519),
    ("narrow_space_thousands", 44),
    ("nbsp_thousands", 44),
    ("over_thousand", 256),
    ("plus_spaced", 337),
    ("product_reorder", 276),
    ("rewrite_apart", 50),
    ("rewrite_cancel", 78),
    ("rewrite_expand", 25),
    ("rewrite_factor", 27),
    ("rewrite_radsimp", 17),
    ("rewrite_together", 116),
    ("set_element_changed", 9),
    ("set_reordered", 11),
    ("sign_flipped", 1559),
    ("significant_decimal", 248),
    ("space_thousands", 44),
    ("star_power", 333),
    ("sum_reorder", 206),
    ("thin_space_thousands", 44),
    ("times_thousand", 300),
    ("trailing_period", 1562),
    ("trailing_zero", 408),
    ("tuple_swapped", 183),
    ("unicode_to_ascii", 54),
    ("whitespace_padding", 1562),
    ("wrong_exponent", 175),
    ("wrong_radicand", 37),
];

/// The literal pair count of every divergence class.
///
/// The counts are measured against the live 1.0 checker, not read back from the
/// committed file.
///
/// FIX-D6 moved 56 pairs out of class 3 and 235 pairs out of class 4, and it
/// moved 91 pairs into class 1. The three numbers are one rule (ruling
/// `D6-dec`): a learner decimal that is the exact rounding of the authored value
/// is correct with a notation tag, and a rounding of `pi` or `e` has no exact
/// rational bound, so the checker refuses it (V2) and the pair leaves the
/// comparison. Class 1 was 965, class 3 was 16,554, and class 4 was 355.
pub const CLASS_COUNTS: [(&str, usize); 5] = [
    ("class 1 outside_grammar", 1056),
    ("class 2 prose_expected", 0),
    ("class 3 comparable", 16498),
    ("class 4 documented_divergence", 320),
    ("oracle_silent", 0),
];

/// The literal pair count of every documented divergence reason.
///
/// The reasons come in three groups.
///
/// 1. The two narrowings of the canonical rational form (FIXM2h). Both carry 6
///    pairs, and `print_report` names every one of them, so the M2 plan quotes
///    them by their text. Both mark a correct learner WRONG in 2.0.
/// 2. The four reasons `docs/plans/M2.md` names. This generated set reaches none
///    of them except the float rung: prose never enters the set (the set holds
///    only in-grammar answers), a SymPy name such as `zoo` leaves 2.0
///    undecidable, which is class 1, and no predicate claims a 1.0
///    simplification. All three stay pinned by literal pairs in
///    `crates/core/tests/answer_divergence.rs`.
/// 3. The 1.0 defects and the grammar rulings that this set reaches. Every one of
///    them marks a correct learner WRONG in 1.0, and 2.0 decides it correctly.
///
/// The float rung carried 240 pairs before FIX-D6, and 238 of them came from the
/// `significant_decimal` family that spec section 9.3 names: a rational with no
/// exact decimal, a radical, `pi`, or `e`, against its own value in ten
/// significant digits. Ruling `D6-dec` decides that family. The 240 pairs split
/// 151 / 5 / 84, which [`D6_SPLIT`] pins:
///
/// - 151 are correct with the notation tag, and 1.0 said correct with no tag.
/// - 5 stay wrong: three nested radicals, whose canonical form is a polynomial
///   over a `sqrt` call and not a radical combination, and two FRACTIONS inside
///   the 1.0 tolerance (`1/1001` for `1/1000`). A fraction carries no digit
///   count, so no rounding reads it.
/// - 84 name `pi` or `e`, which no exact rational bound brackets, so the checker
///   refuses them (V2). They are class 1 and carry no reason.
///
/// Two more groups move. 49 pairs that 1.0 graded WRONG are the exact rounding
/// in 2.0: 46 of the `coarse_decimal` family, whose decimal sits outside the
/// 1e-6 tolerance, and 3 mixed numbers, which 1.0 reads as a product (spec
/// section 7.8). Another 7 `coarse_decimal` pairs name `pi`, and both checkers
/// called them wrong before; 2.0 refuses them now, so class 1 grows by 91.
///
/// `crates/core/tests/answer_divergence.rs` and
/// `crates/core/tests/answer_decimal.rs` pin one pair of each shape.
pub const REASON_COUNTS: [(&str, usize); 16] = [
    ("no polynomial GCD (V1 narrowing)", 6),
    ("no radical rationalization (V1 narrowing)", 6),
    ("no float tolerance rung (D6)", 5),
    ("the exact rounding carries the notation tag (D6-dec)", 151),
    ("an exact rounding 1.0 refused is correct (D6-dec)", 49),
    ("a transcendental identity is not simplified (V1)", 0),
    ("prose is not a value (V2)", 0),
    ("a SymPy name is not a value (V2)", 0),
    (
        "the 1.0 exponent-tower guard refuses a legal power (spec 5.1)",
        11,
    ),
    (
        "the 1.0 tokenizer reads a Python number literal (spec 3.1)",
        8,
    ),
    (
        "the 1.0 radical rewrite misses a nested group (spec 2.2)",
        1,
    ),
    ("a chained inequality raises inside 1.0 (spec 7.7)", 6),
    (
        "the 1.0 rewriter deletes a backslash and leaves a brace group (spec 7.7)",
        3,
    ),
    (
        "the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)",
        46,
    ),
    (
        "2.0 reads a spaced `x` as the times sign (review 1, finding 18)",
        24,
    ),
    (
        "the juxtaposed argument stops at a function name (review 3, finding 5)",
        4,
    ),
];

/// Build one corpus row for a generator test.
pub fn probe_row(answer: &str, shape: &str) -> Row {
    let source = normalize(answer).source;
    let ast = parse(&source).unwrap_or_else(|e| panic!("parse {answer:?}: {e}"));
    let printed = print_ast(&ast, PREC_LOWEST);
    Row {
        answer: answer.to_string(),
        source,
        printed,
        ast,
        canon: canonical_form(answer).ok(),
        shape: shape.to_string(),
        kind: AnswerKind::Expression,
    }
}

/// Build one pair for a predicate test.
pub fn probe_pair(expected: &str, learner: &str, shape: &str) -> Pair {
    Pair {
        generator: "probe",
        intent: Intent::Same,
        expected: expected.to_string(),
        learner: learner.to_string(),
        kind: AnswerKind::Expression,
        shape: shape.to_string(),
    }
}

/// The literal split of the 240 pairs the 1.0 float rung once carried (`D6-dec`).
///
/// The counts are `(rounded, wrong, undecidable)`. Every pair of the class is a
/// pair 1.0 graded True inside its 1e-6 tolerance and 2.0 graded False before
/// FIX-D6.
pub const D6_SPLIT: (usize, usize, usize) = (151, 5, 84);

/// The refusals the rounding rule of `D6-dec` writes, and no other rung writes.
///
/// A pair that carries one of them left the class because of the rule. Every
/// other refusal was already there before the rule, which is class 1.
pub const ROUNDING_REFUSALS: [&str; 4] = [
    "a rounding of a constant is not decidable",
    "a rounding of a negative radicand is not decidable",
    "the value holds more roots than the rounding bound",
    "the rounding needs a finer bound than the checker builds",
];
