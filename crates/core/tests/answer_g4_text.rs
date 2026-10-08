//! Grader pass 4, text: juxtaposed names, grammar the reader lacked, answers
//! written inside a sentence, and label spellings. Each accepted row has its
//! wrong twin where one exists.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const EXACT: &str = r#"{"kind":"exact"}"#;
const SET: &str = r#"{"kind":"set"}"#;

#[derive(Debug, PartialEq, Eq)]
enum V {
    Right,
    Wrong,
    Ungraded,
}

fn verdict(json: &str, key: &str, learner: &str) -> V {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(v) if v.correct => V::Right,
        Outcome::Decided(_) => V::Wrong,
        Outcome::Undecidable(_) => V::Ungraded,
    }
}

fn rows(json: &str, key: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(verdict(json, key, learner), V::Right, "{key} / {learner}");
    }
    for learner in wrong {
        assert_eq!(verdict(json, key, learner), V::Wrong, "{key} / {learner}");
    }
}

fn ungraded(json: &str, key: &str, learners: &[&str]) {
    for learner in learners {
        assert_eq!(
            verdict(json, key, learner),
            V::Ungraded,
            "{key} / {learner}"
        );
    }
}

fn function(vars: &[&str]) -> String {
    let names: Vec<String> = vars.iter().map(|v| format!("\"{v}\"")).collect();
    format!(r#"{{"kind":"function","vars":[{}]}}"#, names.join(","))
}

fn label(options: &[&[&str]]) -> String {
    let options: Vec<String> = options
        .iter()
        .map(|aliases| {
            let aliases: Vec<String> = aliases.iter().map(|a| format!("{a:?}")).collect();
            format!("[{}]", aliases.join(","))
        })
        .collect();
    format!(r#"{{"kind":"label","options":[{}]}}"#, options.join(","))
}

// ---- lexer-juxtaposition (rows 110, 134, 197, 199, 213, 216, 220, 221, 224, 243)

#[test]
fn e_after_a_variable_is_a_product_with_the_exponential() {
    let x = function(&["x"]);
    rows(
        &x,
        "x*e^x",
        &["xe^x", "x e^x", "xe^(x)", "e^x x"],
        &["xe^2", "x e", "xe^(x+1)"],
    );
    rows(
        &function(&["x", "y"]),
        "y*e^(x*y)",
        &["ye^{xy}", "ye^(xy)"],
        &["ye^{x}", "xe^{xy}"],
    );
    rows(
        &function(&["x", "y"]),
        "2*x*e^y",
        &["2xe^y"],
        &["2xe^x", "2ye^x"],
    );
    rows(
        &function(&["t"]),
        "5*e^(4*t) - 2*t*e^(4*t)",
        &["5e^(4t) - 2te^(4t)"],
        &["5e^(4t) - 2te^(3t)", "5e^(4t) + 2te^(4t)"],
    );
}

#[test]
fn pi_and_greek_names_stand_in_a_product() {
    let x = function(&["x"]);
    rows(&x, "pi*x", &["πx", "pix", "x π"], &["πx^2", "2πx"]);
    rows(&function(&["t"]), "pi*t/15", &["πt/15"], &["πt/16", "t/15"]);
    rows(
        &function(&["r", "theta"]),
        "r^2*cos(theta)",
        &["r^2cosθ", "r^2 cos(θ)"],
        &["r^2sinθ", "r^2cos(2θ)"],
    );
    rows(
        EXACT,
        "pi r^2 h/3",
        &["πr²h/3", "pi r^2 h / 3"],
        &["πr²h/2", "πr²h"],
    );
}

#[test]
fn a_function_name_after_a_variable_takes_its_bracket() {
    let t = function(&["t"]);
    rows(
        &t,
        "t*cos(t)",
        &["tcos(t)", "t cos t"],
        &["tsin(t)", "cos(t)"],
    );
    rows(&t, "ln(t)*t", &["tln(t)", "t ln t"], &["tln(2t)"]);
}

#[test]
fn a_capital_variable_of_the_item_joins_a_product() {
    rows(
        &function(&["T", "V"]),
        "ln(T*V)",
        &["ln(TV)", "ln(T V)"],
        &["ln(T+V)", "ln(T)"],
    );
    rows(&function(&["s", "Y"]), "s*Y", &["sY", "Ys"], &["s+Y"]);
}

#[test]
fn a_word_is_never_read_as_a_product() {
    ungraded(
        &function(&["x", "y"]),
        "x*y",
        &["yes", "none", "the", "since", "sees", "arcsecx"],
    );
    ungraded(EXACT, "x", &["xx", "DNE"]);
}

#[test]
fn a_fraction_command_takes_one_character_for_each_unbraced_argument() {
    rows(
        EXACT,
        "1/5",
        &["\\frac15", "\\frac{1}{5}", "\\frac 1 5"],
        &["\\frac14"],
    );
    rows(
        EXACT,
        "1/2",
        &["\\frac12", "\\frac1{2}", "\\frac{1}2"],
        &["\\frac13"],
    );
    rows(EXACT, "x/2", &["\\frac x2", "\\frac{x}{2}"], &["\\frac x3"]);
}

#[test]
fn a_font_command_leaves_the_value_of_its_body() {
    rows(
        EXACT,
        "i",
        &["\\mathbf{i}", "\\mathrm{i}"],
        &["\\mathbf{j}"],
    );
    rows(EXACT, "R", &["\\mathbb{R}"], &["\\mathbb{Q}"]);
}

// ---- grammar-missing

#[test]
fn inverse_reciprocal_functions_read_as_inverse_functions() {
    rows(
        EXACT,
        "arcsec(2)",
        &["arcsec(2)", "acos(1/2)", "pi/3", "π/3"],
        &["pi/6", "arcsec(3)", "acos(2)"],
    );
    rows(EXACT, "pi/6", &["arccsc(2)", "asin(1/2)"], &["arcsec(2)"]);
    rows(EXACT, "pi/3", &["arcsec(2)"], &["arccsc(2)"]);
}

#[test]
fn a_prime_belongs_to_the_name_before_it() {
    rows(EXACT, "not x", &["x'", "not x", "x′"], &["x", "y'", "x''"]);
    rows(
        EXACT,
        "x'y+z",
        &["x'y+z", "z+x'y", "y x' + z"],
        &["x'z+y", "xy+z"],
    );
    rows(EXACT, "x'", &["not x"], &["not y"]);
}

#[test]
fn a_differential_is_one_symbol() {
    rows(
        EXACT,
        "dy/dx",
        &["dy/dx", "dy / dx"],
        &["dx/dy", "y/x", "d*y/(d*x)"],
    );
    rows(
        EXACT,
        "3x^2 dx",
        &["3x^2 dx", "3x^2dx", "dy = 3x^2 dx"],
        &["3x^2 dy", "3x^3 dx", "dy = 3x dx"],
    );
    rows(
        &function(&["x", "dx"]),
        "2*x*dx",
        &["2x dx", "dx 2x"],
        &["2x"],
    );
}

#[test]
fn a_growth_rate_is_a_call_the_grammar_keeps_whole() {
    rows(
        EXACT,
        "Theta(n^2)",
        &["Theta(n^2)", "Theta(n²)"],
        &["Theta(n^3)", "Omega(n^2)"],
    );
    rows(EXACT, "Omega(n)", &["Omega(n)"], &["Theta(n)"]);
}

#[test]
fn a_subscript_reads_in_braces_and_in_brackets() {
    rows(
        EXACT,
        "a_{n-1} + 3",
        &["a_{n-1} + 3", "a_(n-1) + 3", "3 + a_{n-1}"],
        &["a_{n-1} + 2", "a_{n+1} + 3", "a_n + 3"],
    );
    rows(
        EXACT,
        "x_1 + x_2",
        &["x_1 + x_2", "x_{1} + x_{2}", "x_2 + x_1"],
        &["x_1 + x_3"],
    );
    ungraded(EXACT, "x_1 + x_2", &["x1 + x2"]);
}

#[test]
fn a_logarithm_base_reads_as_a_subscript_or_as_digits() {
    let x = function(&["x"]);
    rows(
        &x,
        "log(x, 2)",
        &[
            "log_2(x)",
            "log_2 x",
            "log2(x)",
            "log_{2}(x)",
            "ln(x)/ln(2)",
        ],
        &["log_3(x)", "log(x)", "ln(x)"],
    );
    rows(
        &function(&["x", "b"]),
        "log(x, b)",
        &["log_b(x)", "log_b x", "ln(x)/ln(b)"],
        &["log_x(b)", "log(x)"],
    );
}

#[test]
fn a_bar_pair_is_an_absolute_value() {
    let x = function(&["x"]);
    rows(
        &x,
        "ln(abs(x))",
        &["ln|x|", "ln(|x|)", "ln |x|", "ln(abs(x))"],
        &["log|x|", "ln(x)", "ln|x+1|"],
    );
}

#[test]
fn a_matrix_of_roots_compares_entry_by_entry() {
    let matrix = r#"{"kind":"matrix","rows":2,"cols":2}"#;
    rows(
        matrix,
        "[[1/sqrt(2),1/sqrt(2)],[-1/sqrt(2),1/sqrt(2)]]",
        &[
            "[[1/sqrt(2),1/sqrt(2)],[-1/sqrt(2),1/sqrt(2)]]",
            "[[sqrt(2)/2,sqrt(2)/2],[-sqrt(2)/2,sqrt(2)/2]]",
        ],
        &["[[sqrt(2)/2,sqrt(2)/2],[sqrt(2)/2,sqrt(2)/2]]"],
    );
    rows(
        matrix,
        "[[sqrt(2),0],[0,sqrt(2)]]",
        &["[[sqrt(2),0],[0,sqrt(2)]]", "[[2^(1/2),0],[0,sqrt(2)]]"],
        &["[[sqrt(3),0],[0,sqrt(2)]]", "[[1,0],[0,1]]"],
    );
}

// ---- sentence-answers

#[test]
fn a_value_inside_a_short_sentence_is_graded() {
    rows(
        EXACT,
        "1/2",
        &[
            "the limit is 1/2",
            "The limit is 1/2.",
            "the limit = 1/2",
            "it is 1/2",
        ],
        &["the limit is 1/3"],
    );
    rows(
        EXACT,
        "0",
        &["The horizontal asymptote is y = 0", "the asymptote is 0"],
        &["The horizontal asymptote is y = 1"],
    );
    // A sentence that holds a negation or a second value is another answer.
    rows(EXACT, "1/2", &[], &["the limit is 1/2 or 1/3"]);
    ungraded(EXACT, "1/2", &["the limit is not 1/2"]);
}

#[test]
fn number_words_are_numbers() {
    rows(
        EXACT,
        "2/3",
        &["two thirds", "Two thirds.", "the answer is two thirds"],
        &["two fifths", "three thirds"],
    );
    rows(
        EXACT,
        "12",
        &["twelve", "the answer is twelve"],
        &["eleven"],
    );
    rows(
        EXACT,
        "-1/2",
        &["minus a half", "negative one half"],
        &["one half"],
    );
    rows(
        EXACT,
        "125",
        &["one hundred and twenty five"],
        &["one hundred and twenty"],
    );
}

#[test]
fn an_approximate_mark_leaves_the_value() {
    let metre = r#"{"kind":"unit","quantity":"length","unit":"m"}"#;
    rows(metre, "3 m", &["≈ 3 m", "~3 m", "about 3 m"], &["≈ 4 m"]);
}

#[test]
fn a_few_plain_words_may_stand_before_a_set() {
    rows(
        SET,
        "{1, 2, 3}",
        &["the committee {1, 2, 3}", "{1, 2, 3}"],
        &["the committee {1, 2, 4}"],
    );
}

// ---- label-aliases

#[test]
fn a_label_ignores_underscores_spaces_and_the_spelling_of_a_product_sign() {
    let groups = label(&[&["Z_2×Z_2"], &["Z_4"]]);
    rows(
        &groups,
        "Z_2×Z_2",
        &["Z2xZ2", "Z_2 × Z_2", "z_2×z_2", "Z_2 x Z_2", "Z_2*Z_2"],
        &["Z_4", "Z4"],
    );
    let lines = label(&[&["y = 2x"], &["y = x"]]);
    rows(
        &lines,
        "y = 2x",
        &["y=2x", "y = 2*x", "Y = 2x", "the y = 2x"],
        &["y = x", "y=x"],
    );
    let rings = label(&[&["Z[x]"], &["Q[x]"]]);
    rows(
        &rings,
        "Z[x]",
        &["$\\mathbb{Z}[x]$", "Z[x]"],
        &["$\\mathbb{Q}[x]$"],
    );
}

#[test]
fn a_label_takes_a_leading_verdict_and_a_trailing_explanation() {
    let yes_no = label(&[&["yes"], &["no"]]);
    rows(
        &yes_no,
        "yes",
        &[
            "yes, because the group is abelian and the order does not matter at all here",
            "yes the chain is valid",
            "Yes. It is.",
        ],
        &["no, because it fails", "no it does not factor"],
    );
    rows(
        &label(&[&["no"], &["yes"]]),
        "no",
        &["no it does not factor", "no, x^2+1 has the root 2"],
        &["yes it factors"],
    );
    let steps = label(&[&["Step 3"], &["Step 4"]]);
    rows(
        &steps,
        "Step 3",
        &["Step 3 is not valid"],
        &["Step 4 is not valid"],
    );
    rows(
        &yes_no,
        "yes",
        &[],
        &["yes or no", "yes but no", "no, yes it does"],
    );
}

#[test]
fn a_spelling_that_differs_in_case_alone_is_one_alias() {
    let options = label(&[&["Q", "q"], &["R"]]);
    rows(&options, "Q", &["q", "Q"], &["R", "r"]);
}

// ---- rows routed from the words worker

#[test]
fn glued_capital_names_read_as_symbols_or_products() {
    rows(EXACT, "Pt", &["Pt", "P*t", "tP", "P t"], &["Pr", "P+t"]);
    rows(EXACT, "pA", &["pA", "p*A"], &["pB"]);
    // Two capitals stay outside the grammar without the item variables, so `HT`
    // and `TH` never become one product.
    ungraded(EXACT, "x", &["DV", "HT"]);
    ungraded(EXACT, "x", &["sY(s)"]);
}

#[test]
fn a_factorial_sign_after_a_whole_number_is_its_value() {
    rows(EXACT, "5040", &["7!", "5040"], &["8!", "6!"]);
    rows(EXACT, "1", &["0!", "1!"], &["2!"]);
    ungraded(EXACT, "5040", &["21!", "x!"]);
}

#[test]
fn a_spoken_inverse_is_a_power_of_minus_one() {
    rows(
        EXACT,
        "g^(-1)",
        &["g inverse", "the inverse g^{-1}", "the inverse of g"],
        &["h inverse", "g"],
    );
}

// ---- negative twins found by check_keys in the stabilize pass

#[test]
fn a_label_that_starts_a_longer_alias_and_changes_it_is_a_different_object() {
    let label = r#"{"kind":"label","options":[["the empty function into {0,1}","the empty function","empty function"],["the identity function on {0,1}","the identity"],["no object is initial","none"]]}"#;
    let key = "the empty function into {0,1}";
    for learner in [
        "the empty function into {0,1}",
        "the empty function",
        "the empty function, because nothing maps in",
    ] {
        assert_eq!(verdict(label, key, learner), V::Right, "{learner}");
    }
    for learner in [
        "the empty function into {1,1}",
        "the empty function into {0,2}",
        "the empty function into {1}",
    ] {
        assert_ne!(verdict(label, key, learner), V::Right, "{learner}");
    }
}

#[test]
fn a_multipart_label_with_a_digit_is_not_a_free_word() {
    let multipart = r#"{"kind":"multipart","parts":[{"name":"10_percent","contract":{"kind":"unit","quantity":"volume","unit":"L"}},{"name":"30_percent","contract":{"kind":"unit","quantity":"volume","unit":"L"}}]}"#;
    let key = "10_percent = 2 L; 30_percent = 6 L";
    for learner in [key, "30_percent = 6 L; 10_percent = 2 L", "2 L; 6 L"] {
        assert_eq!(verdict(multipart, key, learner), V::Right, "{learner}");
    }
    for learner in [
        "11_percent = 2 L; 30_percent = 6 L",
        "11_percent = 2 L; 31_percent = 6 L",
        "10_percent = 3 L; 30_percent = 6 L",
    ] {
        assert_ne!(verdict(multipart, key, learner), V::Right, "{learner}");
    }
}
