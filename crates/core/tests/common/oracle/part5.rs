//! Part 5 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

pub fn generate_appended_junk(row: &Row) -> Option<String> {
    Some(format!("{}x", row.answer))
}

/// Every generator of spec section 9.3, in a fixed order.
///
/// The order fixes the pair order, so the generated set is reproducible.
///
/// The array holds every family the spec names except one, and the exception is
/// "word anagram" (`sey` for `yes`). That family applies to prose only, and no
/// prose answer reaches the pair set: `yes` leaves the 2.0 grammar, so the pair
/// is class 1 for every generator, and it measures nothing. The 1.0 anagram
/// defect stays pinned by its literal pair in
/// `crates/core/tests/answer_divergence.rs`.
///
/// M2 review 2, finding 16, showed that this doc comment was false before
/// FIXM2e and FIXM2f: five families were missing, and the 100% class-3
/// agreement measured the generators that were written and not the parity of
/// the checker.
///
/// M2 review 3, finding 14, showed the same gap one level deeper: no family
/// rewrote a rational expression, so the pair set never reached the shape where
/// the two checkers disagree. The six `rewrite_*` families close it.
pub const GENERATORS: [Generator; 44] = [
    Generator {
        name: "identity",
        intent: Intent::Same,
        make: generate_identity,
    },
    Generator {
        name: "whitespace_padding",
        intent: Intent::Same,
        make: generate_padding,
    },
    Generator {
        name: "internal_spaces",
        intent: Intent::Same,
        make: generate_internal_spaces,
    },
    Generator {
        name: "trailing_period",
        intent: Intent::Same,
        make: generate_trailing_period,
    },
    Generator {
        name: "dollar_wrapped",
        intent: Intent::Same,
        make: generate_dollar_wrapped,
    },
    Generator {
        name: "comma_space_removed",
        intent: Intent::Same,
        make: generate_comma_space_removed,
    },
    Generator {
        name: "plus_spaced",
        intent: Intent::Same,
        make: generate_plus_spaced,
    },
    Generator {
        name: "comma_thousands",
        intent: Intent::Same,
        make: generate_comma_thousands,
    },
    Generator {
        name: "space_thousands",
        intent: Intent::Same,
        make: generate_space_thousands,
    },
    Generator {
        name: "nbsp_thousands",
        intent: Intent::Same,
        make: generate_nbsp_thousands,
    },
    Generator {
        name: "narrow_space_thousands",
        intent: Intent::Same,
        make: generate_narrow_space_thousands,
    },
    Generator {
        name: "thin_space_thousands",
        intent: Intent::Same,
        make: generate_thin_space_thousands,
    },
    Generator {
        name: "figure_space_thousands",
        intent: Intent::Same,
        make: generate_figure_space_thousands,
    },
    Generator {
        name: "dot_thousands",
        intent: Intent::Notation,
        make: generate_dot_thousands,
    },
    Generator {
        name: "equivalent_fraction",
        intent: Intent::Same,
        make: generate_equivalent_fraction,
    },
    Generator {
        name: "fraction_to_decimal",
        intent: Intent::Same,
        make: generate_fraction_to_decimal,
    },
    Generator {
        name: "decimal_to_fraction",
        intent: Intent::Same,
        make: generate_decimal_to_fraction,
    },
    Generator {
        name: "trailing_zero",
        intent: Intent::Same,
        make: generate_trailing_zero,
    },
    Generator {
        name: "star_power",
        intent: Intent::Same,
        make: generate_star_power,
    },
    Generator {
        name: "caret_power",
        intent: Intent::Same,
        make: generate_caret_power,
    },
    Generator {
        name: "explicit_multiplication",
        intent: Intent::Same,
        make: generate_explicit_multiplication,
    },
    Generator {
        name: "implicit_multiplication",
        intent: Intent::Same,
        make: generate_implicit_multiplication,
    },
    Generator {
        name: "unicode_to_ascii",
        intent: Intent::Same,
        make: generate_unicode_to_ascii,
    },
    Generator {
        name: "ascii_to_unicode",
        intent: Intent::Same,
        make: generate_ascii_to_unicode,
    },
    Generator {
        name: "sum_reorder",
        intent: Intent::Same,
        make: generate_sum_reorder,
    },
    Generator {
        name: "set_reordered",
        intent: Intent::Same,
        make: generate_set_reordered,
    },
    Generator {
        name: "last_digit_bumped",
        intent: Intent::Different,
        make: generate_last_digit_bumped,
    },
    Generator {
        name: "sign_flipped",
        intent: Intent::Different,
        make: generate_sign_flipped,
    },
    Generator {
        name: "digit_transposition",
        intent: Intent::Different,
        make: generate_digit_transposition,
    },
    Generator {
        name: "times_thousand",
        intent: Intent::Different,
        make: generate_times_thousand,
    },
    Generator {
        name: "over_thousand",
        intent: Intent::Different,
        make: generate_over_thousand,
    },
    Generator {
        name: "coarse_decimal",
        intent: Intent::Different,
        make: generate_coarse_decimal,
    },
    Generator {
        name: "tuple_swapped",
        intent: Intent::Different,
        make: generate_tuple_swapped,
    },
    Generator {
        name: "set_element_changed",
        intent: Intent::Different,
        make: generate_set_element_changed,
    },
    // The four families of spec section 9.3 that M2 review 2, finding 16, names
    // as missing. They come last, so the pair order of every older generator
    // does not move.
    Generator {
        name: "case_flip",
        intent: Intent::Same,
        make: generate_case_flip,
    },
    Generator {
        name: "significant_decimal",
        intent: Intent::Same,
        make: generate_significant_decimal,
    },
    Generator {
        name: "product_reorder",
        intent: Intent::Same,
        make: generate_product_reorder,
    },
    Generator {
        name: "algebraic_refactor",
        intent: Intent::Same,
        make: generate_algebraic_refactor,
    },
    // The rational-rewrite family of M2 review 3, finding #14. Every one of the
    // six is a step a learner performs by hand on an answer with a denominator
    // or a radical, and SymPy writes the spelling. The six come last, so the
    // pair order of every older generator does not move.
    Generator {
        name: "rewrite_together",
        intent: Intent::Same,
        make: generate_rewrite_together,
    },
    Generator {
        name: "rewrite_apart",
        intent: Intent::Same,
        make: generate_rewrite_apart,
    },
    Generator {
        name: "rewrite_cancel",
        intent: Intent::Same,
        make: generate_rewrite_cancel,
    },
    Generator {
        name: "rewrite_factor",
        intent: Intent::Same,
        make: generate_rewrite_factor,
    },
    Generator {
        name: "rewrite_expand",
        intent: Intent::Same,
        make: generate_rewrite_expand,
    },
    Generator {
        name: "rewrite_radsimp",
        intent: Intent::Same,
        make: generate_rewrite_radsimp,
    },
];

/// The two generators that hunt a wrong radicand and a wrong exponent, and the
/// junk generator. They live apart from [`GENERATORS`] only because the array
/// length is a literal; the pair builder runs both arrays in order.
pub const MORE_GENERATORS: [Generator; 3] = [
    Generator {
        name: "wrong_radicand",
        intent: Intent::Different,
        make: generate_wrong_radicand,
    },
    Generator {
        name: "wrong_exponent",
        intent: Intent::Different,
        make: generate_wrong_exponent,
    },
    Generator {
        name: "appended_junk",
        intent: Intent::Different,
        make: generate_appended_junk,
    },
];

// ---------------------------------------------------------------------------
// The generated pair set
// ---------------------------------------------------------------------------
/// One generated pair.
#[derive(Clone)]
pub struct Pair {
    pub generator: &'static str,
    pub intent: Intent,
    pub expected: String,
    pub learner: String,
    pub kind: AnswerKind,
    pub shape: String,
}

/// Build the generated pair set.
///
/// The set is deduplicated by (expected, learner, kind), because the corpus
/// repeats an answer and two generators can meet on one variant. When the set is
/// larger than [`PAIR_CAP`], a seeded shuffle picks the survivors and the
/// survivors go back into generation order.
pub fn generated_pairs() -> Vec<Pair> {
    let rows = in_grammar_rows();
    let mut seen: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    let mut pairs: Vec<Pair> = Vec::new();
    for row in &rows {
        for generator in GENERATORS.iter().chain(MORE_GENERATORS.iter()) {
            let Some(learner) = (generator.make)(row) else {
                continue;
            };
            let key = (row.answer.clone(), learner.clone(), row.kind.as_str());
            if !seen.insert(key) {
                continue;
            }
            pairs.push(Pair {
                generator: generator.name,
                intent: generator.intent,
                expected: row.answer.clone(),
                learner,
                kind: row.kind,
                shape: row.shape.clone(),
            });
        }
    }
    if pairs.len() <= PAIR_CAP {
        return pairs;
    }
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    let mut rng = Rng(SHUFFLE_SEED);
    for index in (1..order.len()).rev() {
        let swap = (rng.next() % (index as u64 + 1)) as usize;
        order.swap(index, swap);
    }
    order.truncate(PAIR_CAP);
    let kept: BTreeSet<usize> = order.iter().copied().collect();
    // The cap is a task bound, not a measurement. A dropped pair is a pair the
    // parity report never asks the oracle about, so the harness names every one
    // of them and it names the total.
    println!(
        "the pair cap dropped {} of {} pairs",
        pairs.len() - kept.len(),
        pairs.len()
    );
    for (index, pair) in pairs.iter().enumerate() {
        if !kept.contains(&index) {
            println!(
                "CAP DROPPED {}: {:?} against {:?} on {}",
                pair.generator,
                pair.expected,
                pair.learner,
                pair.kind.as_str()
            );
        }
    }
    order.sort_unstable();
    order
        .into_iter()
        .filter_map(|index| pairs.get(index).cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// The recorded 1.0 verdicts
// ---------------------------------------------------------------------------
/// One line of `oracle_verdicts_1_0.jsonl`.
#[derive(serde::Deserialize)]
pub struct OracleLine {
    pub expected: String,
    pub learner: String,
    pub kind: String,
    pub equivalent: Option<bool>,
    pub notation: Option<bool>,
    pub timeout: bool,
}

/// The 1.0 verdict on one pair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OracleVerdict {
    pub equivalent: bool,
    pub notation: bool,
}

/// The key of one pair in the verdict map.
pub type PairKey = (String, String, String);

/// Read the committed 1.0 verdicts, keyed by (expected, learner, kind).
pub fn committed_verdicts() -> BTreeMap<PairKey, Option<OracleVerdict>> {
    let path = fixture("oracle_verdicts_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut map = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: OracleLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let verdict = if row.timeout {
            None
        } else {
            Some(OracleVerdict {
                equivalent: row.equivalent.unwrap_or_else(|| {
                    panic!("a recorded verdict with no timeout must carry `equivalent`: {line}")
                }),
                notation: row.notation.unwrap_or_else(|| {
                    panic!("a recorded verdict with no timeout must carry `notation`: {line}")
                }),
            })
        };
        map.insert((row.expected, row.learner, row.kind), verdict);
    }
    map
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------
/// The divergence class of one pair (spec section 9.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Class {
    /// Class 1 — outside the grammar, so 2.0 refuses a verdict (V2).
    OutsideGrammar,
    /// Class 2 — the authored answer is prose, so the topic is mis-kinded (V2).
    ProseExpected,
    /// Class 3 — both sides decidable; a difference here is a 2.0 bug (R5).
    Comparable,
    /// Class 4 — a documented, intentional 2.0 divergence.
    DocumentedDivergence,
    /// The 1.0 oracle timed out, so it has no opinion (spec section 9.1).
    OracleSilent,
}

impl Class {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OutsideGrammar => "class 1 outside_grammar",
            Self::ProseExpected => "class 2 prose_expected",
            Self::Comparable => "class 3 comparable",
            Self::DocumentedDivergence => "class 4 documented_divergence",
            Self::OracleSilent => "oracle_silent",
        }
    }
}
