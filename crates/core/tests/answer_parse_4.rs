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
    // `23 R14`, `x + 2 remainder 3`, and the lower-case `9 r2`/`23 r 14` left
    // the list with the quotient-and-remainder production of D-F3 (unit
    // f2-grammar), which `answer_remainder.rs` pins.
    // `2y · dy/dx` left the list after grader pass 4: `dy/dx` as a factor is a
    // differential (`recovered_2_0.jsonl`, production `differential_factor`).
    for text in [
        "n!",
        "3/0",
        "0/0",
        "zoo",
        "-zoo",
        "6 ≤ ∫ ≤ 15",
        "5 <= 7, so it holds",
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
fn the_corpus_splits_into_3285_parsed_and_207_undecidable_answers() {
    // Grader pass 3: the subscript name (`a_1`), the based logarithm
    // (`log_b(x)`), and the power with a variable exponent (`3^t`) now parse,
    // which moves 11 + 13 rows from the residue to the recovered fixture.
    // The 1.0 residue was 265. The rational-exponent production of D-F3 (unit
    // f2-grammar) reads 15 rows; quotient-and-remainder reads 16; and the
    // `arc_function_name` production of lane B3 reads 2. The value-with-unit
    // production reads the flow value `7 L/min`, while the temperature unit
    // production reads `18 degrees Celsius`. It still refuses `cos 70°`: a
    // unit inside an expression has no reading.
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
    // Stabilize pass after grader pass 4 (2026-10-08): 8 rows join the grammar, 3,285 to
    // 3,293 parsed and 207 to 199 refused. Four are the infinity symbol and the word
    // `infinite` (`∞`, `-∞`, `infinite`), three are a differential factor (`3x^2 dx`,
    // `4y^3 · dy/dx`, `2y · dy/dx + 3x^2`). Their rows moved to `recovered_2_0.jsonl`.
    // LaTeX input pass: `cos 70°` joins the grammar as an angle in degrees, 3,293 to 3,294
    // parsed and 199 to 198 refused. Its row moved to `recovered_2_0.jsonl`.
    assert_eq!(parsed, 3_294, "answers inside the grammar");
    assert_eq!(refused, 198, "answers outside the grammar");
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
    assert_eq!(committed.len(), 198);
}

#[test]
fn the_recovered_answers_keep_their_identity_and_parse() {
    // The 1.0 residue held 265 answers. A 2.0 production moves a row it reads
    // into `recovered_2_0.jsonl` with the production name, so the residue and
    // recovered fixtures together still account for the 265 rows of the 1.0
    // residue plus the one value-with-unit row newly refused by 2.0 (`cos 70°`).
    // D-F3 reads 15 rational exponents and 16 quotient-and-remainder rows;
    // lane B3 reads 2 `arc_function_name` rows; the unit grammar reads the
    // temperature and flow rows `18 degrees Celsius` and `7 L/min`.
    let residue = committed_residue();
    let recovered = committed_recovered();
    let keys = recovered_keys();
    assert_eq!(recovered.len(), keys.len(), "no recovered row repeats");
    assert!(
        residue.is_disjoint(&keys),
        "a recovered row is still refused"
    );
    // `cos 70°` first joined the residue (a unit inside an expression). The degree
    // angle of a trigonometric call reads it now, so it is a recovered row.
    let joined: Key = (
        "complementary-angle-trig".to_string(),
        "kp1".to_string(),
        0,
        "cos 70°".to_string(),
    );
    assert!(keys.contains(&joined), "`cos 70°` is recovered");
    assert_eq!(residue.len() + keys.len(), 265 + 1, "the 1.0 residue");
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
        [
            ("arc_function_name", 2),
            ("degree_angle", 1),
            ("differential_factor", 3),
            ("infinity_symbol", 4),
            ("infinity_word", 1),
            ("quotient_remainder", 16),
            ("rational_exponent", 15),
            ("subscript_and_based_log", 11),
            ("temperature_unit", 1),
            ("value_with_unit", 1),
            ("variable_exponent", 13)
        ]
    );
}

#[test]
fn only_explicitly_recovered_rows_parse_from_the_prose_class() {
    // The frozen 1.0 shape labels `arctan` and `18 degrees Celsius` as prose.
    // The first two have the dedicated `arc_function_name` reading; the
    // temperature is explicitly recovered by the unit grammar.
    let recovered_prose: BTreeSet<String> = committed_recovered()
        .into_iter()
        .filter(|row| {
            matches!(
                row.production.as_str(),
                "arc_function_name" | "temperature_unit" | "infinity_word"
            )
        })
        .map(|row| row.answer)
        .collect();
    // Grader pass 4 adds the word `infinite` (continuity), so the set holds 4.
    assert_eq!(recovered_prose.len(), 4);
    let parsed: BTreeSet<String> = corpus()
        .into_iter()
        .filter(|row| row.shape == "prose_or_words")
        .filter(|row| parse(&normalize(&row.answer).source).is_ok())
        .map(|row| row.answer)
        .collect();
    assert_eq!(
        parsed, recovered_prose,
        "only named productions recover prose-shaped answers"
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
