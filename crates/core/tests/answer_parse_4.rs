//! Part 4 of the `answer_parse` tests. The header of `answer_parse_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::parse::*;

// ---------------------------------------------------------------------------
// V2 — everything outside the grammar is undecidable
// ---------------------------------------------------------------------------
#[test]
fn the_prose_class_never_parses() {
    for text in [
        "yes",
        "sey",
        "no",
        "even",
        "neve",
        "diverges",
        "DNE",
        "undefined",
        "all real numbers",
        "perpendicular",
        "18 degrees Celsius",
        "vertices",
        "sides",
        "true",
        "false",
        "prime",
        "binomial",
        "III",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable (spec section 7.6)"
        );
    }
}

#[test]
fn out_of_grammar_shapes_never_parse() {
    // `xy` left this list in the M2 fix wave: a short run of variable letters is
    // the product `x*y` now. `a_letter_run_the_grammar_does_not_own_stays_
    // undecidable` holds the runs that stay outside the grammar. `9 R2`,
    // `23 R14`, and `x + 2 remainder 3` left the list with the
    // quotient-and-remainder production of D-F3 (unit f2-grammar), which
    // `answer_remainder.rs` pins.
    for text in [
        "9 r2",
        "log_b(x)",
        "n!",
        "3/0",
        "0/0",
        "∞",
        "-∞",
        "zoo",
        "-zoo",
        "6 ≤ ∫ ≤ 15",
        "2y · dy/dx",
        "5 <= 7, so it holds",
        "$3a_1 - a_2$",
        "",
        "   ",
        "$",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn the_evaluation_bounds_refuse_the_1_0_exponent_bombs() {
    assert_eq!(
        parse(&normalize("9^9^9").source).unwrap_err().reason,
        "a tower of powers"
    );
    assert_eq!(
        parse(&normalize("9**9**9**9").source).unwrap_err().reason,
        "a tower of powers"
    );
    assert_eq!(
        parse(&normalize("2**10000000").source).unwrap_err().reason,
        "an exponent outside the evaluation bound"
    );
    assert_eq!(
        parse(&normalize("(2)**(9999999)").source)
            .unwrap_err()
            .reason,
        "an exponent outside the evaluation bound"
    );
    assert!(parse(&normalize("2**1000").source).is_ok());
    assert!(parse(&normalize("2**1001").source).is_err());
}

#[test]
fn the_rce_payloads_of_1_0_never_parse() {
    for text in [
        "__import__('os').system('touch /tmp/_rce_marker_should_not_exist')",
        "exec(\"open('/tmp/_rce_marker_should_not_exist','w').write('x')\")",
        "eval(\"__import__('os').getenv('ANTHROPIC_API_KEY')\")",
        "print(open('.env.example').read())",
        "integrate(exp(-x**2),(x,0,oo))",
        "factorint(9)",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn the_input_cap_refuses_a_longer_answer() {
    let inside = "1".repeat(MAX_ANSWER_CHARS);
    assert_eq!(MAX_ANSWER_CHARS, 4_000);
    assert!(parse(&inside).is_ok());
    let outside = "1".repeat(MAX_ANSWER_CHARS + 1);
    assert_eq!(
        parse(&outside).unwrap_err().reason,
        "the answer is longer than the input cap"
    );
}

#[test]
fn deep_nesting_is_refused_and_never_overflows_the_stack() {
    let deep = format!("{}1{}", "(".repeat(1_500), ")".repeat(1_500));
    assert_eq!(
        parse(&deep).unwrap_err().reason,
        "the answer nests too deeply"
    );
    let shallow = format!("{}1{}", "(".repeat(10), ")".repeat(10));
    assert_eq!(parse(&shallow).unwrap(), int(1));
}

#[test]
fn the_corpus_splits_into_3258_parsed_and_234_undecidable_answers() {
    // The 1.0 residue was 265. The rational-exponent production of D-F3 (unit
    // f2-grammar) reads 15 of those rows and the quotient-and-remainder
    // production reads 16, which `recovered_2_0.jsonl` names.
    let rows = corpus();
    assert_eq!(rows.len(), 3_492, "corpus size");
    let mut parsed = 0_usize;
    let mut refused = 0_usize;
    for row in &rows {
        if parse(&normalize(&row.answer).source).is_ok() {
            parsed += 1;
        } else {
            refused += 1;
        }
    }
    assert_eq!(parsed, 3_258, "answers inside the grammar");
    assert_eq!(refused, 234, "answers outside the grammar");
}

#[test]
fn every_shape_gives_its_literal_parse_count() {
    let rows = corpus();
    for (shape, want_parsed, want_refused) in SHAPE_COUNTS {
        let mut parsed = 0_usize;
        let mut refused = 0_usize;
        for row in rows.iter().filter(|row| row.shape == shape) {
            if parse(&normalize(&row.answer).source).is_ok() {
                parsed += 1;
            } else {
                refused += 1;
            }
        }
        assert_eq!(parsed, want_parsed, "{shape}: parsed");
        assert_eq!(refused, want_refused, "{shape}: refused");
    }
}

#[test]
fn the_undecidable_answers_are_exactly_the_committed_fixture() {
    let measured: BTreeSet<Key> = corpus()
        .into_iter()
        .filter(|row| parse(&normalize(&row.answer).source).is_err())
        .map(|row| (row.topic_id, row.kp_id, row.exemplar_index, row.answer))
        .collect();
    let committed = committed_residue();
    let missing: Vec<&Key> = committed.difference(&measured).collect();
    let extra: Vec<&Key> = measured.difference(&committed).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the residue moved: missing {missing:?}, extra {extra:?}"
    );
    assert_eq!(committed.len(), 234);
}

#[test]
fn the_recovered_answers_keep_their_identity_and_parse() {
    // The 1.0 residue held 265 answers. A 2.0 production moves a row it reads
    // into `recovered_2_0.jsonl` with the production name, so the two fixtures
    // together are still the 265 rows of the 1.0 residue. The productions of
    // D-F3 (unit f2-grammar): `rational_exponent` reads 15 rows, and
    // `quotient_remainder` reads 16 rows.
    let residue = committed_residue();
    let recovered = committed_recovered();
    let keys = recovered_keys();
    assert_eq!(recovered.len(), keys.len(), "no recovered row repeats");
    assert!(
        residue.is_disjoint(&keys),
        "a recovered row is still refused"
    );
    assert_eq!(residue.len() + keys.len(), 265, "the 1.0 residue");
    let mut per_production: std::collections::BTreeMap<&str, usize> = Default::default();
    for row in &recovered {
        assert!(
            parse(&normalize(&row.answer).source).is_ok(),
            "{:?} is in the recovered fixture and the grammar refuses it",
            row.answer
        );
        *per_production.entry(row.production.as_str()).or_insert(0) += 1;
    }
    let counts: Vec<(&str, usize)> = per_production.into_iter().collect();
    assert_eq!(
        counts,
        [("quotient_remainder", 16), ("rational_exponent", 15)]
    );
}

#[test]
fn no_answer_of_the_prose_class_claims_a_verdict() {
    let parsed: Vec<String> = corpus()
        .into_iter()
        .filter(|row| row.shape == "prose_or_words")
        .filter(|row| parse(&normalize(&row.answer).source).is_ok())
        .map(|row| row.answer)
        .collect();
    assert!(
        parsed.is_empty(),
        "C4: prose must never reach a deterministic verdict, but {parsed:?} parsed"
    );
}

#[test]
fn ten_seconds_of_random_input_never_panics() {
    // The alphabet mixes the grammar's own characters with the glyphs of the V4
    // table, so the generator reaches the productions and not only the reject path.
    let alphabet: Vec<char> = "0123456789+-*/^()[]{},.<>= xyzabcnE\\$%!_'\"\
         πτ∞·−–≤≥θαβλ½⅓⅔¼¾°√²³⁴"
        .chars()
        .collect();
    let mut rng = Rng(0x2026_0826_4d32_5531);
    let start = std::time::Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        for _ in 0..1_000 {
            let length = (rng.next() % 80) as usize;
            let text: String = if rng.next().is_multiple_of(2) {
                (0..length)
                    .map(|_| {
                        let index = (rng.next() as usize) % alphabet.len();
                        alphabet.get(index).copied().unwrap_or('0')
                    })
                    .collect()
            } else {
                let bytes: Vec<u8> = (0..length).map(|_| (rng.next() % 256) as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let normalized = normalize(&text);
            let _ = parse(&normalized.source);
            let _ = parse(&text);
            cases += 1;
        }
    }
    assert!(cases > 1_000, "the fuzz ran {cases} cases");
}
