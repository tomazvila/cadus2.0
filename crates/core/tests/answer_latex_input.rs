//! LaTeX spellings read as the plain construct, and trigonometric functions at
//! exact special angles read as their value.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

#[derive(Debug, PartialEq, Eq)]
enum V {
    Right,
    Wrong,
    Ungraded,
}

fn graded(contract: &str, key: &str, learner: &str) -> V {
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(v) if v.correct => V::Right,
        Outcome::Decided(_) => V::Wrong,
        Outcome::Undecidable(_) => V::Ungraded,
    }
}

fn verdict(key: &str, learner: &str) -> V {
    graded(r#"{"kind":"exact"}"#, key, learner)
}

fn all_right(key: &str, learners: &[&str]) {
    for learner in learners {
        assert_eq!(verdict(key, learner), V::Right, "{key} vs {learner}");
    }
}

#[test]
fn trig_values_at_special_angles_read_exactly() {
    // (name, angle as a multiple of pi, value) over one turn in steps of 30 and 45 degrees.
    let table: [(&str, &str, &str); 44] = [
        ("sin", "0", "0"),
        ("sin", "pi/6", "1/2"),
        ("sin", "pi/4", "sqrt(2)/2"),
        ("sin", "pi/3", "sqrt(3)/2"),
        ("sin", "pi/2", "1"),
        ("sin", "2*pi/3", "sqrt(3)/2"),
        ("sin", "3*pi/4", "sqrt(2)/2"),
        ("sin", "5*pi/6", "1/2"),
        ("sin", "pi", "0"),
        ("sin", "7*pi/6", "-1/2"),
        ("sin", "5*pi/4", "-sqrt(2)/2"),
        ("sin", "4*pi/3", "-sqrt(3)/2"),
        ("sin", "3*pi/2", "-1"),
        ("sin", "-pi/6", "-1/2"),
        ("sin", "2*pi", "0"),
        ("cos", "0", "1"),
        ("cos", "pi/6", "sqrt(3)/2"),
        ("cos", "pi/4", "sqrt(2)/2"),
        ("cos", "pi/3", "1/2"),
        ("cos", "pi/2", "0"),
        ("cos", "2*pi/3", "-1/2"),
        ("cos", "3*pi/4", "-sqrt(2)/2"),
        ("cos", "5*pi/6", "-sqrt(3)/2"),
        ("cos", "pi", "-1"),
        ("cos", "5*pi/3", "1/2"),
        ("cos", "7*pi/3", "1/2"),
        ("cos", "-pi", "-1"),
        ("tan", "0", "0"),
        ("tan", "pi/6", "sqrt(3)/3"),
        ("tan", "pi/4", "1"),
        ("tan", "pi/3", "sqrt(3)"),
        ("tan", "2*pi/3", "-sqrt(3)"),
        ("tan", "3*pi/4", "-1"),
        ("tan", "pi", "0"),
        ("sec", "0", "1"),
        ("sec", "pi/3", "2"),
        ("sec", "pi/4", "sqrt(2)"),
        ("sec", "pi", "-1"),
        ("csc", "pi/2", "1"),
        ("csc", "pi/6", "2"),
        ("csc", "pi/4", "sqrt(2)"),
        ("cot", "pi/4", "1"),
        ("cot", "pi/6", "sqrt(3)"),
        ("cot", "pi/2", "0"),
    ];
    for (name, angle, value) in table {
        assert_eq!(
            verdict(value, &format!("{name}({angle})")),
            V::Right,
            "{name}({angle}) = {value}"
        );
    }
}

#[test]
fn trig_values_that_are_undefined_or_symbolic_stay_unevaluated() {
    assert_eq!(verdict("1", "tan(pi/2)"), V::Wrong);
    assert_eq!(verdict("0", "sec(pi/2)"), V::Wrong);
    assert_eq!(verdict("0", "csc(0)"), V::Wrong);
    assert_eq!(verdict("sin(x)", "sin(x)"), V::Right);
    assert_eq!(verdict("sin(x)", "cos(x)"), V::Wrong);
    assert_eq!(verdict("1", "sin(x)"), V::Wrong);
    assert_eq!(verdict("1/2", "sin(pi/12)"), V::Wrong);
}

#[test]
fn the_named_trig_probes_are_right() {
    all_right(
        "1",
        &[
            "sin(pi/2)",
            r"\sin(\pi/2)",
            r"\sin\left(\frac{\pi}{2}\right)",
        ],
    );
    all_right("-1", &["cos(pi)", r"\cos(\pi)"]);
    all_right("1", &["cos(0)"]);
    all_right("1/2", &["sin(pi/6)", r"\sin\left(\frac{\pi}{6}\right)"]);
}

#[test]
fn degrees_inside_a_trig_call_read_as_an_angle() {
    all_right(
        "1/2",
        &[
            r"\cos(60^\circ)",
            "cos(60°)",
            r"\cos 60^\circ",
            "sin 30°",
            r"\sin 30^\circ",
            r"\sin 30^{\circ}",
            r"\sin\left(30^\circ\right)",
        ],
    );
    all_right("sqrt(3)", &["tan(60°)"]);
    all_right("-1", &["cos(180°)"]);
    // A plain number is radians, and a wrong angle stays wrong.
    assert_eq!(verdict("1/2", "sin(30)"), V::Wrong);
    assert_eq!(verdict("1/2", "cos(30°)"), V::Wrong);
}

#[test]
fn a_wrong_value_stays_wrong() {
    assert_eq!(verdict("1/2", "sin(pi/3)"), V::Wrong);
    assert_eq!(verdict("1/2", r"\frac{1}{3}"), V::Wrong);
    assert_eq!(verdict("{1,3}", r"\{1,2\}"), V::Wrong);
    assert_eq!(verdict("1/2", r"\dfrac{1}{3}"), V::Wrong);
    assert_eq!(verdict("1205", r"1{,}206"), V::Wrong);
}

#[test]
fn fraction_spellings_read_as_a_fraction() {
    all_right(
        "1/2",
        &[
            r"\dfrac{1}{2}",
            r"\tfrac{1}{2}",
            r"\frac12",
            r"\frac{1}2",
            r"\frac1{2}",
        ],
    );
}

#[test]
fn infinity_reads_in_both_spellings() {
    all_right("-infinity", &[r"-\infty"]);
    all_right("oo", &[r"\infty", "∞"]);
    assert_eq!(
        graded(
            r#"{"kind":"inequality_union"}"#,
            "(-oo, 3)",
            r"(-\infty, 3)"
        ),
        V::Right
    );
    assert_eq!(verdict("oo", r"-\infty"), V::Wrong);
}

#[test]
fn text_inside_an_answer_reads_as_its_words() {
    all_right("3, 4", &[r"3\text{ or }4", r"3\text{ and }4"]);
    all_right("5 km", &[r"5\text{ km}", r"5 \text{km}"]);
}

#[test]
fn braces_of_a_set_read_with_or_without_the_backslash() {
    all_right("{1,2}", &[r"\{1,2\}", r"\{2, 1\}", "{1,2}"]);
}

#[test]
fn a_thousands_group_in_math_mode_is_one_number() {
    all_right("1205", &[r"1{,}205", "1,205"]);
    all_right("1234567", &[r"1{,}234{,}567"]);
}

#[test]
fn spacing_commands_leave_no_token() {
    all_right("3", &[r"1\,+\;2\!", r"1\: + 2"]);
    all_right("1000", &[r"1\,000"]);
}

#[test]
fn word_commands_read_as_letters() {
    all_right("sin(x)", &[r"\operatorname{sin}(x)", r"\sin(x)"]);
    all_right("dx", &[r"\mathrm{d}x"]);
}

#[test]
fn matrices_read_under_the_exact_and_matrix_contracts() {
    let matrix = r#"{"kind":"matrix","rows":2,"cols":2}"#;
    for learner in [
        r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
        r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}",
        r"\begin{matrix}1&2\\3&4\end{matrix}",
    ] {
        assert_eq!(verdict("[[1,2],[3,4]]", learner), V::Right, "{learner}");
        assert_eq!(
            graded(matrix, "[[1,2],[3,4]]", learner),
            V::Right,
            "{learner}"
        );
    }
    let wrong = r"\begin{pmatrix} 1 & 2 \\ 3 & 5 \end{pmatrix}";
    assert_eq!(verdict("[[1,2],[3,4]]", wrong), V::Wrong);
    assert_eq!(graded(matrix, "[[1,2],[3,4]]", wrong), V::Wrong);
}

#[test]
fn a_text_without_these_spellings_is_unchanged() {
    assert_eq!(verdict("3", "3"), V::Right);
    assert_eq!(verdict("3", r"3\text{ or }"), V::Ungraded);
}

#[test]
fn a_fraction_exponent_reads_as_a_rational_power() {
    all_right("x^(1/2)", &[r"x^{\frac{1}{2}}", "x^{1/2}"]);
    all_right("sqrt(x)", &[r"x^{\frac{1}{2}}"]);
    assert_eq!(verdict("x^(1/3)", r"x^{\frac{1}{2}}"), V::Wrong);
}

#[test]
fn a_bare_degree_circle_is_a_degree_sign() {
    all_right("90°", &[r"90^\circ", r"90^{\circ}", "90°"]);
    assert_eq!(verdict("90°", r"80^\circ"), V::Wrong);
    let angle = r#"{"kind":"unit","quantity":"angle","unit":"°"}"#;
    assert_eq!(graded(angle, "90°", r"90^\circ"), V::Right);
    assert_eq!(graded(angle, "90°", r"91^\circ"), V::Wrong);
}

#[test]
fn not_equal_spellings_read_as_not_equal() {
    let union = r#"{"kind":"inequality_union"}"#;
    for learner in [r"x \ne 3", r"x \neq 3", "x != 3", "x ≠ 3"] {
        assert_eq!(verdict("x != 3", learner), V::Right, "{learner}");
        assert_eq!(graded(union, "x != 3", learner), V::Right, "{learner}");
    }
    assert_eq!(verdict("x != 3", r"x \ne 4"), V::Wrong);
}

#[test]
fn angle_brackets_read_as_a_vector() {
    let point = r#"{"kind":"coordinates","arity":2}"#;
    assert_eq!(graded(point, "(1, 2)", r"\langle 1, 2 \rangle"), V::Right);
    assert_eq!(graded(point, "(1, 2)", r"\langle 1, 3 \rangle"), V::Wrong);
}
