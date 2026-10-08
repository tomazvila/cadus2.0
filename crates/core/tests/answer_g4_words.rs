//! Grader pass 4, WORDS worker: ordered products and the "other" rows.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

/// "correct", "wrong" or "ungraded" for one key, contract and learner answer.
fn verdict(contract: &str, key: &str, learner: &str) -> &'static str {
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(v) if v.correct => "correct",
        Outcome::Decided(_) => "wrong",
        Outcome::Undecidable(_) => "ungraded",
    }
}

const WORD: &str = r#"{"kind":"ordered_word"}"#;
const EXACT: &str = r#"{"kind":"exact"}"#;

fn cases(contract: &str, rows: &[(&str, &str, &str)]) {
    for (key, learner, want) in rows {
        assert_eq!(
            verdict(contract, key, learner),
            *want,
            "key {key:?} learner {learner:?}"
        );
    }
}

#[test]
fn a_word_keeps_the_order_of_its_factors() {
    cases(
        WORD,
        &[
            ("ab", "ab", "correct"),
            ("ab", "a*b", "correct"),
            ("ab", "a b", "correct"),
            ("ab", "a·b", "correct"),
            ("ab", "ba", "wrong"),
            ("ab", "b a", "wrong"),
            ("r^2 s", "r^2 s", "correct"),
            ("r^2 s", "r^{2}s", "correct"),
            ("r^2 s", "rrs", "correct"),
            ("r^2 s", "s r^2", "wrong"),
            ("r^2 s", "s r", "wrong"),
            ("a^(-1)*c*b^(-1)", "b^(-1)ca^(-1)", "wrong"),
            ("a^(-1)*c*b^(-1)", "a^-1 c b^-1", "correct"),
            ("a^(-1)*c*b^(-1)", "a^{-1}cb^{-1}", "correct"),
        ],
    );
}

#[test]
fn matrix_products_and_inverses_follow_the_group_laws() {
    cases(
        WORD,
        &[
            ("A^{-1}B", "B A^-1", "wrong"),
            ("A^{-1}B", "A^-1 B", "correct"),
            ("A^{-1}B", "A⁻¹B", "correct"),
            ("B^-1 A^-1", "(AB)^-1", "correct"),
            ("(AB)^-1", "B^-1 A^-1", "correct"),
            ("(AB)^-1", "A^-1 B^-1", "wrong"),
            ("AC", "AC", "correct"),
            ("AC", "CA", "wrong"),
            ("AC", "ac", "wrong"),
            ("a b b^-1", "a", "correct"),
        ],
    );
}

#[test]
fn a_matrix_size_keeps_its_order() {
    cases(
        WORD,
        &[
            ("4 x 3", "4 x 3", "correct"),
            ("4 x 3", "4x3", "correct"),
            ("4 x 3", "4 by 3", "correct"),
            ("4 x 3", "4 × 3", "correct"),
            ("4 x 3", "3 x 4", "wrong"),
            ("4 x 3", "12", "wrong"),
        ],
    );
}

#[test]
fn products_of_cycles_compose_as_permutations() {
    cases(
        WORD,
        &[
            ("(1 2 3)", "(1 2 3)", "correct"),
            ("(1 2 3)", "(2 3 1)", "correct"),
            ("(1 2 3)", "(1 3 2)", "wrong"),
            ("(1 2)(3 4)", "(1 2)(3 4)", "correct"),
            ("(1 2)(3 4)", "(3 4)(1 2)", "correct"),
            ("(1 2)(2 3)", "(2 3)(1 2)", "wrong"),
            ("(1 2)(3 4)", "(1 2)", "wrong"),
        ],
    );
}

#[test]
fn an_unreadable_word_is_left_ungraded() {
    cases(
        WORD,
        &[("ab", "a + b", "ungraded"), ("ab", "a^x", "ungraded")],
    );
}

#[test]
fn exact_stays_commutative() {
    cases(EXACT, &[("ab", "ba", "correct"), ("xy", "yx", "correct")]);
}

#[test]
fn the_mutants_of_a_word_are_other_words() {
    let mutants = cadus_core::answer::contract::word_mutants("r^2 s");
    assert!(!mutants.is_empty());
    for mutant in mutants {
        assert_eq!(verdict(WORD, "r^2 s", &mutant), "wrong", "{mutant}");
    }
}

// ---- the "other" rows ----

#[test]
fn a_no_solution_key_reads_the_common_words() {
    cases(
        EXACT,
        &[
            ("no solution", "no solution", "correct"),
            ("no solution", "none", "correct"),
            ("no solution", "No real solutions.", "correct"),
            ("no solution", "∅", "correct"),
            ("no solution", "5", "wrong"),
            ("no solution", "x = 2", "wrong"),
            ("no solution", "all real numbers", "wrong"),
            ("no solution", "dne", "wrong"),
            ("DNE", "does not exist", "correct"),
            ("DNE", "no solution", "wrong"),
        ],
    );
}

#[test]
fn a_ratio_may_be_written_with_a_slash_or_to() {
    let ratio = r#"{"kind":"reduced_ratio"}"#;
    cases(
        ratio,
        &[
            ("4:25", "4:25", "correct"),
            ("4:25", "4/25", "correct"),
            ("4:25", "4 to 25", "correct"),
            ("4:25", "8/50", "wrong"),
            ("4:25", "5/25", "wrong"),
            ("4:25", "25:4", "wrong"),
            ("4:25", "25/4", "wrong"),
        ],
    );
}

#[test]
fn an_ascending_chain_may_hold_a_root() {
    let chain = r#"{"kind":"ascending_chain"}"#;
    cases(
        chain,
        &[
            ("5 < sqrt(30) < 6", "5 < √30 < 6", "correct"),
            ("5 < sqrt(30) < 6", "5 < sqrt(30) < 6", "correct"),
            ("5 < sqrt(30) < 6", "5 < sqrt(31) < 6", "wrong"),
            ("1 < 2 < 3", "1 < 2 < 3", "correct"),
        ],
    );
}

#[test]
fn a_single_power_may_carry_its_name() {
    let power = r#"{"kind":"required_single_power"}"#;
    cases(
        power,
        &[
            ("3^4", "x = 3^4", "correct"),
            ("3^4", "3^4", "correct"),
            ("3^4", "81", "wrong"),
            ("3^4", "x = 81", "wrong"),
            ("3^4", "x = 9^2", "wrong"),
        ],
    );
}

#[test]
fn a_quotient_may_be_spoken() {
    let quotient = r#"{"kind":"quotient_remainder"}"#;
    cases(
        quotient,
        &[
            ("9 R3", "9 R3", "correct"),
            ("9 R3", "9 boxes and 3 left over", "correct"),
            ("9 R3", "quotient 9, remainder 3", "correct"),
            ("9 R3", "9 with 3 remaining", "correct"),
            ("9 R3", "9 boxes and 4 left over", "wrong"),
            ("9 R3", "3 boxes and 9 left over", "wrong"),
        ],
    );
}

#[test]
fn a_matrix_may_be_written_in_rows_or_latex() {
    let two_by_three = r#"{"kind":"matrix","rows":2,"cols":3}"#;
    let square = r#"{"kind":"matrix","rows":2,"cols":2}"#;
    cases(
        two_by_three,
        &[
            ("[[0,1,0],[1,0,1]]", "0 1 0; 1 0 1", "correct"),
            ("[[0,1,0],[1,0,1]]", "[0 1 0; 1 0 1]", "correct"),
            ("[[0,1,0],[1,0,1]]", "[0, 1, 0; 1, 0, 1]", "correct"),
            ("[[0,1,0],[1,0,1]]", "0 1 0; 1 1 1", "wrong"),
        ],
    );
    cases(
        square,
        &[
            (
                "[[1,2],[3,4]]",
                r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
                "correct",
            ),
            (
                "[[1,2],[3,4]]",
                r"\begin{bmatrix}1&2\\3&4\end{bmatrix}",
                "correct",
            ),
            (
                "[[1,2],[3,4]]",
                r"\begin{pmatrix}1&2\\3&5\end{pmatrix}",
                "wrong",
            ),
            ("[[1,2],[3,4]]", r"\begin{pmatrix}1&2\end{pmatrix}", "wrong"),
        ],
    );
}

#[test]
fn a_scalar_multiple_of_the_key_grid_is_correct() {
    let contract = r#"{"kind":"scalar_multiple","rows":1,"cols":3}"#;
    cases(
        contract,
        &[
            ("[1,-1,-3]", "[1,-1,-3]", "correct"),
            ("[1,-1,-3]", "[-1,1,3]", "correct"),
            ("[1,-1,-3]", "[2,-2,-6]", "correct"),
            ("[1,-1,-3]", "[1/2,-1/2,-3/2]", "correct"),
            ("[1,-1,-3]", "[0,0,0]", "wrong"),
            ("[1,-1,-3]", "[1,-1,-2]", "wrong"),
            ("[1,-1,-3]", "[2,1,-6]", "wrong"),
        ],
    );
}

#[test]
fn a_function_form_keeps_the_outer_operation() {
    let form = |var: &str| format!(r#"{{"kind":"function_form","vars":["{var}"]}}"#);
    cases(
        &form("x"),
        &[
            ("(x+1)*(x-2)", "(x-2)(x+1)", "correct"),
            ("(x+1)*(x-2)", "x^2-x-2", "wrong"),
            ("(x+1)*(x-2)", "(x+1)(x-3)", "wrong"),
            ("2sin(4x)cos(x)", "sin 5x + sin 3x", "wrong"),
            ("1/(x-1) + 1/(x+1)", "2x/(x^2-1)", "wrong"),
            ("1/(x-1) + 1/(x+1)", "1/(x+1) + 1/(x-1)", "correct"),
        ],
    );
    cases(
        &form("t"),
        &[
            ("cos(t)", "sin(t+pi/2)", "wrong"),
            ("cos(t)", "cos(-t)", "correct"),
        ],
    );
}

#[test]
fn a_property_answer_with_a_unit_needs_the_item_unit() {
    let bare =
        r#"{"kind":"property","check":"integer_in_range","args":{"min":"1000","max":"2000"}}"#;
    let metres = r#"{"kind":"property","check":"integer_in_range","args":{"min":"1000","max":"2000","unit":"m"}}"#;
    cases(
        bare,
        &[
            ("1500", "1500", "correct"),
            ("1500", "1500 m", "ungraded"),
            ("1500", "2500", "wrong"),
        ],
    );
    cases(
        metres,
        &[
            ("1500", "1500", "correct"),
            ("1500", "1500 m", "correct"),
            ("1500", "1.5 km", "correct"),
            ("1500", "2500 m", "wrong"),
            ("1500", "1500 s", "ungraded"),
        ],
    );
}
