//! Part 3 of the `template` tests. The header of `template_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::template::*;

/// The bounded draw never returns the bound, and it repeats from its seed.
///
/// The draw uses Lemire's multiply-and-shift, so no `%` runs on a drawn value
/// and the low end of a domain is not over-represented
/// (`docs/plans/M4.md`, the fixed RNG decision).
#[test]
fn the_bounded_draw_stays_below_the_bound_and_repeats_from_the_seed() {
    let mut rng = rng_from_seed(1);
    let first: Vec<u64> = (0..8).map(|_| below(&mut rng, 12)).collect();
    let mut again = rng_from_seed(1);
    let second: Vec<u64> = (0..8).map(|_| below(&mut again, 12)).collect();
    assert_eq!(first, second, "the seed alone decides the stream");

    let mut different = rng_from_seed(2);
    let other: Vec<u64> = (0..8).map(|_| below(&mut different, 12)).collect();
    assert_ne!(first, other, "a different seed gives a different stream");

    // A bound of one has one answer, and a bound of zero has the same answer.
    let mut edge = rng_from_seed(3);
    assert_eq!(below(&mut edge, 1), 0);
    assert_eq!(below(&mut edge, 0), 0);

    // Over 120,000 draws of a 12-value domain every value comes out, and none of
    // them comes out more than 12 percent of the time. A `% range` fold on a
    // range that does not divide 2^64 leans on the low values; 10,000 is the
    // even share and 12,000 is the bound this asserts.
    let mut counts = [0_usize; 12];
    let mut spread = rng_from_seed(9);
    for _ in 0..120_000 {
        let drawn = below(&mut spread, 12);
        assert!(drawn < 12, "the draw returned {drawn}");
        let index = usize::try_from(drawn).expect("the value fits");
        if let Some(slot) = counts.get_mut(index) {
            *slot += 1;
        }
    }
    for (value, count) in counts.iter().enumerate() {
        assert!(
            (8_000..=12_000).contains(count),
            "value {value} came out {count} times"
        );
    }
}

/// The candidate stream walks the whole space once at or under the limit.
///
/// 1.0 shuffles the full product and yields every tuple once
/// (`problem_templates.py:353-375`). Avoidance is then exact: with 11 of 12
/// instances blocked, the walk still reaches the free one.
#[test]
fn the_candidate_stream_walks_the_whole_small_space_once() {
    let doc = doc_from(&perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(5);
    let stream = compiled.candidates(&mut rng).expect("the stream builds");

    assert_eq!(stream.len(), 12);
    let mut rendered: BTreeSet<String> = BTreeSet::new();
    for bindings in &stream {
        rendered.insert(
            compiled
                .instantiate(bindings.clone())
                .expect("it instantiates")
                .text,
        );
    }
    assert_eq!(rendered.len(), 12);
    assert!(rendered.contains("Compute $1^{2}$."));
    assert!(rendered.contains("Compute $12^{2}$."));

    // The walk is shuffled, so two seeds give two orders of the same 12 tuples.
    let mut other = rng_from_seed(6);
    let second = compiled.candidates(&mut other).expect("the stream builds");
    assert_eq!(second.len(), 12);
    assert_ne!(stream, second, "the walk is shuffled");
}

/// A constraint set no tuple satisfies is an error, and never a violating tuple.
#[test]
fn an_unsatisfiable_constraint_set_reports_that_it_found_no_tuple() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "impossible", "answer_kind": "numeric",
            "statement": "Compute ${a} - {b}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 5},
                       "b": {"kind": "int", "low": 8, "high": 12}},
            "constraints": [{"op": "gt", "left": "a", "right": "b"}],
            "answer_expr": "a - b", "hints": ["Compare the two numbers."],
            "samples": [{"params": {"a": 1, "b": 8}, "expected": "-7"}]}"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(13);

    let error = compiled
        .plan()
        .draw_satisfying(&doc.constraints, &mut rng)
        .expect_err("no tuple has a > b");
    assert_eq!(
        error.to_string(),
        "no tuple of the declared domains satisfies the constraints after 1000 draw(s)"
    );

    // The exhaustive stream is empty rather than wrong.
    assert!(compiled.candidates(&mut rng).expect("it builds").is_empty());
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(0)
    );
}

// --------------------------------------------------------------------------
// The document
// --------------------------------------------------------------------------
/// The body round-trips byte for byte, and an unknown key refuses it.
///
/// `content_store.digest` covers the whole body (spec section 8, trap 8), so a
/// re-serialized document that changed one byte would ask for a new human
/// approval it does not need.
#[test]
fn the_document_round_trips_and_refuses_an_unknown_key() {
    let compact = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"constraints":[{"op":"gt","left":{"add":["a",{"lit":1}]},"right":{"lit":100}}],"answer_expr":"a**2","solution_sketch":"Square it.","hints":["What does squaring mean?"],"samples":[{"params":{"a":1},"expected":"1"}],"space_size":12}"#;
    let doc = doc_from(compact);
    assert_eq!(to_body(&doc).expect("it writes"), compact);
    assert_eq!(doc.v, 1);
    assert_eq!(doc.space_size, Some(SpaceSize::Exact(12)));
    assert_eq!(doc.topic_id, "perfect-squares");

    // An unknown key is a rejection, and never a silently dropped instruction.
    let extra = compact.replace(r#""v":1"#, r#""v":1,"difficulty":3"#);
    let error = from_body(&extra).expect_err("`difficulty` is not a field");
    assert!(error.to_string().contains("unknown field"), "{error}");

    // An unknown domain kind is a rejection too.
    let kind = compact.replace(r#""kind":"int""#, r#""kind":"gaussian""#);
    assert!(from_body(&kind).is_err());

    // An unknown constraint operator is a rejection.
    let op = compact.replace(r#""op":"gt""#, r#""op":"approximately""#);
    assert!(from_body(&op).is_err());

    // A malformed term is a rejection: `sub` takes exactly two operands.
    let sub = compact.replace(r#"{"add":["a",{"lit":1}]}"#, r#"{"sub":["a"]}"#);
    let sub_error = from_body(&sub).expect_err("`sub` takes two operands");
    assert!(
        sub_error
            .to_string()
            .contains("a sub term needs exactly 2 operands, and it has 1"),
        "{sub_error}"
    );

    // A JSON float is a rejection: a decimal literal is written as a string (D6).
    let float = compact.replace(r#"{"lit":100}"#, r#"{"lit":1.5}"#);
    assert!(from_body(&float).is_err());

    // The name sets the document reports are the sets a gate reads.
    assert_eq!(
        doc.param_names().into_iter().collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
    assert_eq!(
        doc.constraint_names().into_iter().collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
    assert_eq!(
        doc.statement_names()
            .expect("the statement scans")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
}

/// A sample carries the tuple it was worked on, and the answer a human wrote.
#[test]
fn a_sample_binds_a_tuple_and_an_answer() {
    let doc = doc_from(&perfect_squares_body());
    assert_eq!(doc.samples.len(), 2);
    let first = doc.samples.first().expect("the first sample");
    assert_eq!(first.expected.text(), "1");
    let bindings = first.bindings();
    assert_eq!(whole_values(&bindings), vec![1]);

    let compiled = Compiled::new(&doc).expect("the template compiles");
    for sample in &doc.samples {
        let instance = compiled
            .instantiate(sample.bindings())
            .expect("the sample instantiates");
        assert_eq!(
            instance.answer,
            sample.expected.text(),
            "the sample and the expression agree"
        );
    }
}

/// A domain past `MAX_DOMAIN_SIZE` is refused, and never materialized.
///
/// The payload is 1.0's, `low 1, high 10**6`
/// (`tests/test_problem_templates.py:169` through the specification, section 9).
#[test]
fn a_domain_past_the_bound_is_refused() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "too-wide", "answer_kind": "numeric",
            "statement": "Compute ${a}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 1000000}},
            "answer_expr": "a", "hints": ["Read the number."],
            "samples": [{"params": {"a": 1}, "expected": "1"}]}"#,
    );
    let error = DrawPlan::new(&doc.params).expect_err("a million values is past the bound");
    assert_eq!(
        error.to_string(),
        "domain \"a\" holds 1000000 values, which exceeds MAX_DOMAIN_SIZE (10000)"
    );

    // An empty range is refused too.
    let empty = doc_from(
        r#"{"v": 1, "topic_id": "empty", "answer_kind": "numeric",
            "statement": "Compute ${a}$.",
            "params": {"a": {"kind": "int", "low": 5, "high": 1}},
            "answer_expr": "a", "hints": ["Read the number."],
            "samples": [{"params": {"a": 5}, "expected": "5"}]}"#,
    );
    assert_eq!(
        DrawPlan::new(&empty.params)
            .expect_err("the range runs backwards")
            .to_string(),
        "int domain 5..1 is empty"
    );

    // A rational domain whose denominator range holds zero is refused.
    let zero = doc_from(
        r#"{"v": 1, "topic_id": "zero-denominator", "answer_kind": "numeric",
            "statement": "Compute ${r}$.",
            "params": {"r": {"kind": "rational", "num": {"low": 1, "high": 9},
                             "den": {"low": -3, "high": 3}}},
            "answer_expr": "r", "hints": ["Read the fraction."],
            "samples": [{"params": {"r": "1/2"}, "expected": "1/2"}]}"#,
    );
    assert_eq!(
        DrawPlan::new(&zero.params)
            .expect_err("the denominator range holds zero")
            .to_string(),
        "the denominator range -3..3 of a rational domain holds zero"
    );
}

// --------------------------------------------------------------------------
// The M4 review 1 repairs
// --------------------------------------------------------------------------
/// M4 review 1, finding 8: a decimal constraint literal survives the body.
///
/// `write_rational` writes `1/2` for the literal `0.5`, because 2 is not a power
/// of ten. The reader took decimals only, so a gate-accepted document wrote a
/// body it did not read again, and the refill refused the digest a reviewer
/// had approved (C6).
#[test]
fn a_constraint_literal_round_trips_through_every_form_it_writes() {
    for (written, again) in [
        (r#"{"lit": "0.5"}"#, "1/2"),
        (r#"{"lit": "3/2"}"#, "3/2"),
        (r#"{"lit": "-5/2"}"#, "-5/2"),
        (r#"{"lit": "0.1"}"#, "0.1"),
        (r#"{"lit": 100}"#, "100"),
    ] {
        let source = format!(r#"{{"op": "ge", "left": "p", "right": {written}}}"#);
        let constraint: Constraint = serde_json::from_str(&source).expect("the term reads");
        let body = serde_json::to_string(&constraint).expect("the term writes");
        let reread: Constraint = serde_json::from_str(&body).expect("the written term reads again");
        assert_eq!(constraint, reread, "{written} does not round-trip");
        assert!(
            body.contains(again),
            "{written} writes {body}, which does not carry {again}"
        );
    }

    // The whole document round-trips, which is the property the digest rests on.
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "fraction-of-a-number", "answer_kind": "numeric",
            "statement": "Compute ${p} \\times {a}$.",
            "params": {"p": {"kind": "rational", "num": {"low": 1, "high": 4},
                             "den": {"low": 2, "high": 5}},
                       "a": {"kind": "int", "low": 4, "high": 12}},
            "constraints": [{"op": "ge", "left": "p", "right": {"lit": "0.5"}}],
            "answer_expr": "p*a", "hints": ["What does the denominator ask for?"],
            "samples": [{"params": {"p": "0.5", "a": 4}, "expected": "2"}]}"#,
    );
    let body = to_body(&doc).expect("the document writes");
    assert!(
        body.contains(r#"{"lit":"1/2"}"#),
        "the body writes the literal as a fraction: {body}"
    );
    let again = from_body(&body).expect("the written body reads again");
    assert_eq!(doc, again);
}

/// A literal outside the three forms names the three forms the reader takes.
#[test]
fn a_literal_that_is_not_a_number_names_the_forms_the_reader_takes() {
    let error = serde_json::from_str::<Constraint>(
        r#"{"op": "eq", "left": "a", "right": {"lit": "one half"}}"#,
    )
    .expect_err("a word is not a literal");
    assert!(
        error.to_string().starts_with(
            "a lit term needs a whole number, a decimal string, or 'n/d', not \"one half\""
        ),
        "{error}"
    );
}

/// M4 review 1, finding 9: a value keeps the spelling its author wrote.
///
/// A choice value of `0.2` is the rational 1/5 for the evaluator and the text
/// `0.2` for the renderer. Before the repair the renderer wrote `1/5`, so a
/// decimals knowledge point served fraction problems.
#[test]
fn a_decimal_value_renders_as_the_decimal_its_author_wrote() {
    let value = Scalar::Text("0.2".to_string()).value();
    assert_eq!(value.canonical_string(), "0.2");
    assert_eq!(
        value.as_rational().map(std::string::ToString::to_string),
        Some("1/5".to_string())
    );
    // The number decides equality and order, and never the spelling.
    assert_eq!(
        value,
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(1),
            num_bigint::BigInt::from(5),
        ))
    );

    // A decimal domain writes the same spelling, at the scale it declares.
    let domain: Domain =
        serde_json::from_str(r#"{"kind": "decimal", "low": -2, "high": 21, "scale": 1}"#)
            .expect("the domain reads");
    let values = domain.values("d").expect("the domain walks");
    assert_eq!(values.len(), 24);
    let written: Vec<String> = values.iter().map(Value::canonical_string).collect();
    assert_eq!(written.first().map(String::as_str), Some("-0.2"));
    assert_eq!(written.get(2).map(String::as_str), Some("0.0"));
    assert_eq!(written.get(4).map(String::as_str), Some("0.2"));
    assert_eq!(written.last().map(String::as_str), Some("2.1"));
    assert_eq!(domain.size("d").expect("the domain counts"), 24);

    // A scale past the bound is a domain error, and never a wide number.
    let wide: Domain =
        serde_json::from_str(r#"{"kind": "decimal", "low": 1, "high": 9, "scale": 12}"#)
            .expect("the domain reads");
    assert_eq!(
        wide.values("d")
            .expect_err("the scale is too large")
            .to_string(),
        "decimal domain scale 12 exceeds MAX_DECIMAL_SCALE (9)"
    );
}

/// M4 review 1, finding 18: a value that is not atomic takes brackets.
///
/// The evaluator brackets a negative literal and a fraction under a power. The
/// renderer now writes the same brackets, so the printed problem asks the
/// question the stored answer answers. A decimal and a text choice are atomic
/// and take none.
#[test]
fn the_renderer_brackets_a_negative_value_and_a_fraction() {
    let statement = "Compute ${a}^{{2}}$.";
    assert_eq!(
        render(statement, &bind(&[("a", -3)])).expect("it renders"),
        "Compute $(-3)^{2}$."
    );
    assert_eq!(
        render(statement, &bind(&[("a", 3)])).expect("it renders"),
        "Compute $3^{2}$."
    );

    let mut fraction = Bindings::new();
    fraction.insert(
        "a".to_string(),
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(3),
            num_bigint::BigInt::from(2),
        )),
    );
    assert_eq!(
        render(statement, &fraction).expect("it renders"),
        "Compute $(3/2)^{2}$."
    );

    let mut decimal = Bindings::new();
    decimal.insert("a".to_string(), Scalar::Text("0.2".to_string()).value());
    assert_eq!(
        render(statement, &decimal).expect("it renders"),
        "Compute $0.2^{2}$."
    );

    let mut negative_decimal = Bindings::new();
    negative_decimal.insert("a".to_string(), Scalar::Text("-0.2".to_string()).value());
    assert_eq!(
        render(statement, &negative_decimal).expect("it renders"),
        "Compute $(-0.2)^{2}$."
    );

    let mut text = Bindings::new();
    let (name, value) = text_binding("a", "\\times");
    text.insert(name, value);
    assert_eq!(
        render("Compute $2 {a} 3$.", &text).expect("it renders"),
        "Compute $2 \\times 3$."
    );
}

/// The statement and the answer of one instance ask and answer one question.
///
/// `a**2` with `a = -3` answers 9. The statement must therefore read `(-3)^{2}`
/// and never `-3^{2}`, which is -9 (M4 review 1, finding 18).
#[test]
fn a_negative_instance_states_the_question_its_answer_answers() {
    let doc = doc_from(&squares_of_negatives_body());
    let compiled = Compiled::new(&doc).expect("the template compiles");
    for (value, text, answer) in [
        (-3, "Compute $(-3)^{2}$.", "9"),
        (-12, "Compute $(-12)^{2}$.", "144"),
    ] {
        let instance = compiled
            .instantiate(bind(&[("a", value)]))
            .expect("the tuple instantiates");
        assert_eq!(instance.text, text);
        assert_eq!(instance.answer, answer);
    }
}
