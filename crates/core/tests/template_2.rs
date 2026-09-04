//! Part 2 of the `template` tests. The header of `template_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::template::*;

/// The comparison operators and the number-theory predicates decide exactly.
#[test]
fn the_constraint_operators_decide_on_exact_values() {
    let bindings = bind(&[("a", 12), ("b", 8)]);
    let cases: [(Cmp, bool); 9] = [
        (Cmp::Eq, false),
        (Cmp::Ne, true),
        (Cmp::Lt, false),
        (Cmp::Le, false),
        (Cmp::Gt, true),
        (Cmp::Ge, true),
        // 12 does not divide 8.
        (Cmp::Divides, false),
        // gcd(12, 8) = 4.
        (Cmp::Coprime, false),
        // 2 + 8 = 10 in the ones column.
        (Cmp::Carries, true),
    ];
    for (op, expected) in cases {
        let constraint = Constraint {
            op,
            left: Term::Param("a".to_string()),
            right: Term::Param("b".to_string()),
        };
        assert_eq!(
            holds(&constraint, &bindings).expect("it decides"),
            expected,
            "{} on 12 and 8",
            op.as_str()
        );
    }

    // `divides` reads left into right: 4 divides 12.
    let divides = Constraint {
        op: Cmp::Divides,
        left: Term::Param("b".to_string()),
        right: Term::Param("a".to_string()),
    };
    assert!(holds(&divides, &bind(&[("a", 12), ("b", 4)])).expect("it decides"));
    assert!(!holds(&divides, &bind(&[("a", 12), ("b", 5)])).expect("it decides"));
}

/// The term language computes on exact rationals and refuses a text binding.
#[test]
fn the_term_language_computes_exactly() {
    let body = r#"[
      {"op": "gt", "left": {"add": ["a", "b"]}, "right": {"lit": 100}},
      {"op": "eq", "left": {"digit_sum": "a"}, "right": {"lit": 9}},
      {"op": "eq", "left": {"mod": ["a", {"lit": 7}]}, "right": {"lit": 0}},
      {"op": "eq", "left": {"abs": {"sub": ["b", "a"]}}, "right": {"lit": 18}},
      {"op": "eq", "left": {"mul": ["a", {"lit": 2}]}, "right": {"lit": 126}}
    ]"#;
    let constraints: Vec<Constraint> = serde_json::from_str(body).expect("the terms read");
    assert_eq!(constraints.len(), 5);

    // a = 63: 63 + 45 = 108 > 100; 6 + 3 = 9; 63 mod 7 = 0; |45 - 63| = 18;
    // 63 * 2 = 126.
    let bindings = bind(&[("a", 63), ("b", 45)]);
    for constraint in &constraints {
        assert!(
            holds(constraint, &bindings).expect("it decides"),
            "{:?} holds on a = 63, b = 45",
            constraint.op
        );
    }

    // `mod` takes the sign of the divisor, so -7 mod 3 is 2 and not -1.
    let modulo: Constraint = serde_json::from_str(
        r#"{"op": "eq", "left": {"mod": ["a", {"lit": 3}]}, "right": {"lit": 2}}"#,
    )
    .expect("the term reads");
    assert!(holds(&modulo, &bind(&[("a", -7)])).expect("it decides"));

    // A text choice has no value, so a constraint over it is a document defect.
    let mut text = Bindings::new();
    let (name, value) = text_binding("a", "\\times");
    text.insert(name, value);
    let over_text: Constraint =
        serde_json::from_str(r#"{"op": "gt", "left": "a", "right": {"lit": 1}}"#)
            .expect("the term reads");
    let error = holds(&over_text, &text).expect_err("a text has no value");
    assert_eq!(
        error.to_string(),
        "a constraint term names \"a\", which is bound to the text \"\\\\times\" and not to a number"
    );

    // A decimal literal is written as a string, so no float ever enters (D6).
    let decimal: Constraint =
        serde_json::from_str(r#"{"op": "eq", "left": "a", "right": {"lit": "1.5"}}"#)
            .expect("the term reads");
    let mut half = Bindings::new();
    half.insert(
        "a".to_string(),
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(3),
            num_bigint::BigInt::from(2),
        )),
    );
    assert!(holds(&decimal, &half).expect("it decides"));
}

// --------------------------------------------------------------------------
// The renderer
// --------------------------------------------------------------------------
/// The scanner reads `{{`, `}}`, and `{name}`, and refuses every other brace.
///
/// The rules and the 12-character snippet come from 1.0 `_stray_brace`
/// (`problem_templates.py:231-259`) and the rejection message at `:521`.
#[test]
fn the_scanner_reads_the_placeholder_grammar() {
    let mut bindings = bind(&[("a", 7)]);
    let (name, value) = text_binding("op", "+");
    bindings.insert(name, value);

    // A doubled brace writes one brace, and a placeholder writes the value.
    assert_eq!(
        cadus_core::template::render("Compute ${a}^{{2}}$.", &bindings).expect("it renders"),
        "Compute $7^{2}$."
    );
    // A thousands mark in LaTeX is a doubled brace pair.
    assert_eq!(
        cadus_core::template::render("Is $7{{,}}329$ larger?", &bindings).expect("it renders"),
        "Is $7{,}329$ larger?"
    );
    // A closing doubled brace writes one brace.
    assert_eq!(
        cadus_core::template::render("{{{a}}}", &bindings).expect("it renders"),
        "{7}"
    );

    // A single literal brace is a rejection, and the report names the index and
    // the 12 characters that start there.
    let stray = stray_brace("Compute $7^{2}$ and ${a}$.").expect("the brace is stray");
    assert_eq!(stray.index, 11);
    assert_eq!(stray.snippet, "{2}$ and ${a");

    // The report of a short tail is the tail, and never a panic.
    let tail = stray_brace("value }").expect("the brace is stray");
    assert_eq!(tail.index, 6);
    assert_eq!(tail.snippet, "}");

    // A well-formed statement has no stray brace.
    assert_eq!(stray_brace("Compute ${a}^{{2}}$."), None);

    // A hole no tuple binds refuses the whole statement, so a hole never reaches
    // a learner. 1.0 buys this with `format_map`'s `KeyError` (`:328-336`).
    let missing =
        cadus_core::template::render("Compute ${z}$.", &bindings).expect_err("z is not bound");
    assert_eq!(
        missing,
        RenderError::Undeclared {
            name: "z".to_string()
        }
    );

    // The placeholder grammar is identifiers only: no format spec, no
    // conversion, no attribute access, no index (1.0 `_PLACEHOLDER_RE`).
    for statement in ["{a:>5}", "{a!r}", "{a.real}", "{a[0]}", "{0}", "{}"] {
        assert!(
            stray_brace(statement).is_some(),
            "{statement} must be a stray brace"
        );
    }

    // Every placeholder name of a statement comes out, in name order.
    let names =
        cadus_core::template::placeholders("${a} {op} {b} and {{a}}").expect("the statement scans");
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        vec!["a".to_string(), "b".to_string(), "op".to_string()]
    );
}

// --------------------------------------------------------------------------
// The exact evaluator
// --------------------------------------------------------------------------
/// The evaluation-only functions compute exactly and are erased.
///
/// The set is the one `docs/plans/M4.md` fixes. Every expected answer below is
/// worked by hand.
#[test]
fn the_evaluation_only_functions_compute_exactly_and_disappear() {
    let cases: [EvalCase; 14] = [
        ("gcd(a, b)", &[("a", 12), ("b", 18)], "6"),
        ("lcm(a, b)", &[("a", 4), ("b", 6)], "12"),
        ("floor(a/b)", &[("a", 7), ("b", 2)], "3"),
        ("ceiling(a/b)", &[("a", 7), ("b", 2)], "4"),
        ("min(a, b)", &[("a", 7), ("b", 2)], "2"),
        ("max(a, b)", &[("a", 7), ("b", 2)], "7"),
        ("factorial(a)", &[("a", 5)], "120"),
        ("binomial(a, b)", &[("a", 5), ("b", 2)], "10"),
        ("abs(a - b)", &[("a", 3), ("b", 10)], "7"),
        ("sqrt(a)", &[("a", 49)], "7"),
        ("sqrt(a)", &[("a", 8)], "sqrt(8)"),
        // The radicand of a root is not always whole: the denominator needs the
        // same perfect-square test as the numerator (M4 review 1, finding 14).
        ("sqrt(4/a)", &[("a", 3)], "sqrt(4/3)"),
        ("sqrt(4/a)", &[("a", 9)], "2/3"),
        ("a/b", &[("a", 10), ("b", 4)], "5/2"),
    ];

    for (source, values, expected) in cases {
        let ast = cadus_core::template::parse_answer_expr(source)
            .unwrap_or_else(|error| panic!("{source} parses: {error}"));
        let computed = cadus_core::template::answer(&ast, &bind(values))
            .unwrap_or_else(|error| panic!("{source} evaluates: {error}"));
        assert_eq!(computed.text, expected, "{source} over {values:?}");
        // No answer string carries a name from the evaluation-only set.
        for name in cadus_core::template::EXTRA_FUNCTIONS {
            assert!(
                !computed.text.contains(name),
                "{} still names {name}",
                computed.text
            );
        }
    }
}

/// `gcd` is one function name and never the product `g*c*d`.
///
/// The M2 parser splits a run of up to three plain letters into a product, so
/// `gcd` reads as `g*c*d` without the extra name set. The template parser passes
/// the set, so the run stays one call.
#[test]
fn an_extra_function_name_does_not_split_into_a_letter_run() {
    let with_set = cadus_core::template::parse_answer_expr("gcd(a, b)").expect("it parses");
    assert_eq!(
        with_set,
        cadus_core::answer::Ast::Func(
            "gcd".to_string(),
            vec![
                cadus_core::answer::Ast::Var("a".to_string()),
                cadus_core::answer::Ast::Var("b".to_string())
            ]
        )
    );
    // The M2 grammar alone splits the run into the product `g*c*d`, so the call
    // never reaches the checker as one function. That is why V2 needs the extra
    // set declared and bounded rather than guessed.
    assert!(
        !matches!(
            cadus_core::answer::parse("gcd(a, b)"),
            Ok(cadus_core::answer::Ast::Func(..))
        ),
        "the M2 grammar must not read `gcd` as a function"
    );
}

/// An `answer_expr` outside the grammar refuses the template (V2).
#[test]
fn an_answer_expression_outside_the_grammar_refuses_the_template() {
    // 1.0 admits `Piecewise`, `Eq`, `And`, `simplify`, `sign`, and more
    // (`_ALLOWED_NAMES`, `problem_templates.py:533-548`). None of them is in the
    // 2.0 grammar: `docs/plans/M4.md` picks option (a) of the specification.
    for source in [
        "Piecewise((a, a > b), (b, True))",
        "simplify(a + b)",
        "sign(a - b)",
        "Eq(a, b)",
        "a @ b",
    ] {
        assert!(
            cadus_core::template::parse_answer_expr(source).is_err(),
            "{source} must leave the grammar"
        );
    }

    let body = perfect_squares_body().replace("a**2", "Piecewise((a, a > 1), (1, True))");
    let doc = doc_from(&body);
    let error = Compiled::new(&doc).expect_err("the answer expression leaves the grammar");
    assert!(
        error
            .to_string()
            .starts_with("answer_expr is outside the decidable grammar"),
        "{error}"
    );
}

/// The evaluator refuses the values 1.0 answers with `zoo`, `I`, and `nan`.
///
/// 1.0 does not check finiteness: `1/0` returns `"zoo"` and `sqrt(-4)` returns
/// `"2*I"` (spec section 3.4, step 5). Neither is a number, and 1.0 catches them
/// one rung later with `_NON_ANSWERS` (`:203`). 2.0 refuses them at the source.
#[test]
fn the_evaluator_refuses_a_division_by_zero_and_a_negative_root() {
    let divide = cadus_core::template::parse_answer_expr("a/b").expect("it parses");
    let error = cadus_core::template::answer(&divide, &bind(&[("a", 1), ("b", 0)]))
        .expect_err("1/0 is not a number");
    assert_eq!(error, EvalError::DivideByZero);
    assert_eq!(error.to_string(), "answer_expr divides by zero");

    let root = cadus_core::template::parse_answer_expr("sqrt(a)").expect("it parses");
    let negative = cadus_core::template::answer(&root, &bind(&[("a", -4)]))
        .expect_err("sqrt(-4) is not a real number");
    assert_eq!(
        negative.to_string(),
        "answer_expr takes the square root of the negative number -4"
    );

    // A factorial outside the bound refuses, and never allocates the number.
    let factorial = cadus_core::template::parse_answer_expr("factorial(a)").expect("it parses");
    let too_large = cadus_core::template::answer(&factorial, &bind(&[("a", 201)]))
        .expect_err("201 is past the bound");
    assert_eq!(
        too_large.to_string(),
        "factorial takes a whole number from 0 to 200, and it got 201"
    );
    assert!(cadus_core::template::answer(&factorial, &bind(&[("a", 200)])).is_ok());
}

// --------------------------------------------------------------------------
// The domains, the space, and the draws
// --------------------------------------------------------------------------
/// The pinned constants of the specification, section 9.
///
/// Each one is written out here. A test that read the constant back would pass
/// on a mutated constant, which is exactly the failure `tests/test_problem_templates.py:1136-1140`
/// records: an evaluator changed `GATE_SAMPLES` to 1 and the 1.0 file stayed green.
#[test]
fn the_pinned_constants_hold_their_1_0_values() {
    assert_eq!(EXHAUSTIVE_SPACE_LIMIT, 4_096);
    assert_eq!(MIN_SPACE_SIZE, 12);
    assert_eq!(MAX_DOMAIN_SIZE, 10_000);
    assert_eq!(MAX_CHOICES, 24);
    assert_eq!(RESAMPLE_ATTEMPTS, 24);
    assert_eq!(TEMPLATE_VERSION, 1);
}

/// `space_size` of `a` in 1..12 is 12, with no constraint.
///
/// The literal is `tests/test_problem_templates.py:148` through the
/// specification, section 9.
#[test]
fn the_space_of_the_1_0_fixtures_is_twelve() {
    let doc = doc_from(&perfect_squares_body());
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(12)
    );

    // `6*b` with `b` in 1..12 is 12 as well (`:1315`): the space counts tuples,
    // and never the values the answer takes.
    let six_b = doc_from(
        r#"{"v": 1, "topic_id": "six-times", "answer_kind": "numeric",
            "statement": "Compute $6 \\times {b}$.",
            "params": {"b": {"kind": "int", "low": 1, "high": 12}},
            "answer_expr": "6*b", "hints": ["Count in sixes."],
            "samples": [{"params": {"b": 1}, "expected": "6"}]}"#,
    );
    assert_eq!(
        space_size(&six_b.params, &six_b.constraints).expect("the space counts"),
        SpaceSize::Exact(12)
    );
}

/// Above the exhaustive limit the count is the count the walk found.
///
/// Two domains of 200 values make 40,000 declared tuples, which is the sampled
/// branch of `tests/test_problem_templates.py:1180-1200`. The constraint
/// `a > b` holds on 19,900 of them, worked by hand: 199 + 198 + ... + 1.
///
/// The count is the count of DISTINCT satisfying tuples one walk found, and the
/// walk stops at `GATE_SAMPLES` distinct tuples, so the number is 4,096. It is a
/// floor of the 19,900 and never a scaled guess: the deleted 4,096-draw
/// estimator read 4 satisfying tuples as 244 and 20 as 0 (M4 review 2, findings
/// 2 and 5).
#[test]
fn a_space_above_the_limit_counts_the_tuples_the_walk_found() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "big-space", "answer_kind": "numeric",
            "statement": "Compute ${a} - {b}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 200},
                       "b": {"kind": "int", "low": 1, "high": 200}},
            "constraints": [{"op": "gt", "left": "a", "right": "b"}],
            "answer_expr": "a - b", "hints": ["Subtract the ones column."],
            "samples": [{"params": {"a": 200, "b": 1}, "expected": "199"}]}"#,
    );
    let counted = space_size(&doc.params, &doc.constraints).expect("the space counts");
    assert_eq!(
        counted,
        SpaceSize::Estimated {
            estimate: 4_096,
            samples: 9_371,
            hits: 4_575,
        }
    );
    assert!(!counted.is_exact());
    // The count never runs above the true satisfying count of 19,900. The
    // deleted estimator ran above it and below it.
    assert!(counted.count() <= 19_900, "count = {}", counted.count());
    // The count is a function of the document alone: the seed is a constant, so
    // a reviewer reproduces the stored number.
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        counted
    );
    // The walk that produced the count carries the tuples the gate reads, so the
    // two can never disagree.
    let walked = walk_satisfying(&doc.params, &doc.constraints).expect("the space walks");
    assert_eq!(walked.space, counted);
    assert_eq!(walked.tuples.len(), 4_096);
    assert!(!walked.exhaustive);
    assert_eq!(walked.drawn, Some(9_371));
    let distinct: BTreeSet<Bindings> = walked.tuples.iter().cloned().collect();
    assert_eq!(distinct.len(), 4_096);
}
