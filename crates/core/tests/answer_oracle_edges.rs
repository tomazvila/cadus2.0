//! The helpers of the `answer_oracle` tests that only the live 1.0 oracle reached.
//!
//! The canonical-form walkers get one answer per shape, and the harness helpers
//! run against a fake interpreter: a shell script that prints the ready line and
//! answers every request from a fixed table. The fake stands in for the 1.0
//! interpreter, which only the build box has.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::os::unix::fs::PermissionsExt;

use common::oracle::*;

/// The radical atoms of `answer`, as sorted text.
fn radicals(answer: &str) -> Vec<String> {
    let canon = canonical_form(answer).unwrap_or_else(|e| panic!("{answer}: {e:?}"));
    radical_atoms(&canon).into_iter().collect()
}

/// The variable names of `answer`, as sorted text.
fn names(answer: &str) -> Vec<String> {
    let canon = canonical_form(answer).unwrap_or_else(|e| panic!("{answer}: {e:?}"));
    let mut out = BTreeSet::new();
    collect_variable_names(&canon, &mut out);
    out.into_iter().collect()
}

#[test]
fn the_radical_walk_reaches_every_canonical_shape() {
    let sqrt_x = r#"sqrt([Poly({{Var("x"): 1}: Ratio { numer: 1, denom: 1 }})])**1"#;
    let cases: [(&str, &[&str]); 16] = [
        ("1/2", &[]),
        ("sqrt(2)+1", &["sqrt(2)"]),
        ("x^2+sqrt(2)*x", &["sqrt(2)**1"]),
        ("(x+1)/(x+sqrt(2))", &["sqrt(2)**1"]),
        ("2*sqrt(x)", &[sqrt_x]),
        ("2*sin(sqrt(2))", &["sqrt(2)"]),
        ("e^(sqrt(2))", &["sqrt(2)"]),
        ("pi*x", &[]),
        ("sin(sqrt(2))", &["sqrt(2)"]),
        ("(1, sqrt(2))", &["sqrt(2)"]),
        ("{1, sqrt(3)}", &["sqrt(3)"]),
        ("[1, sqrt(5)]", &["sqrt(5)"]),
        ("-1 <= x <= sqrt(7)", &["sqrt(7)"]),
        ("x > 1", &[]),
        ("x = sqrt(2)", &["sqrt(2)"]),
        ("y = 4", &[]),
    ];
    for (answer, want) in cases {
        assert_eq!(radicals(answer), want, "{answer}");
    }
}

#[test]
fn the_variable_walk_reaches_every_canonical_shape() {
    let cases: [(&str, &[&str]); 16] = [
        ("1/2", &[]),
        ("sqrt(2)", &[]),
        ("x^2+sqrt(2)*x", &["x"]),
        ("(x+1)/(x+2)", &["x"]),
        ("sin(y)/(x+2)", &["x", "y"]),
        ("2*sqrt(x)", &["x"]),
        ("e^x", &["x"]),
        ("pi*x", &["x"]),
        ("sin(x)", &["x"]),
        ("(1, x)", &["x"]),
        ("{y}", &["y"]),
        ("[z, 3]", &["z"]),
        ("-1 <= x <= sqrt(7)", &["x"]),
        ("x < 3", &["x"]),
        ("y = 4", &[]),
        ("x = y", &["y"]),
    ];
    for (answer, want) in cases {
        assert_eq!(names(answer), want, "{answer}");
    }
    assert!(holds_the_variable_x("2*sqrt(x)"));
    assert!(
        !holds_the_variable_x("x^(1/2)"),
        "a refused answer holds no variable"
    );
}

/// One pair of the edge tests: numeric, of the generator `edge`.
fn pair(expected: &str, learner: &str) -> Pair {
    Pair {
        generator: "edge",
        intent: Intent::Same,
        expected: expected.to_string(),
        learner: learner.to_string(),
        kind: AnswerKind::Numeric,
        shape: "integer".to_string(),
    }
}

#[test]
fn the_verdict_line_spells_a_decided_verdict_and_a_timeout() {
    let decided = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    assert_eq!(
        verdict_line(&pair("7329", "7.329"), Some(decided)).to_string(),
        r#"{"equivalent":true,"expected":"7329","kind":"numeric","learner":"7.329","notation":false,"timeout":false}"#
    );
    assert_eq!(
        verdict_line(&pair("(x+1)**200", "x**200+1"), None).to_string(),
        r#"{"equivalent":null,"expected":"(x+1)**200","kind":"numeric","learner":"x**200+1","notation":null,"timeout":true}"#
    );
}

// ---------------------------------------------------------------------------
// The harness helpers, against a fake interpreter
// ---------------------------------------------------------------------------
/// Write one fake interpreter under `target/oracle-fake`, and return its path.
///
/// The script ignores the helper path and the timeout, prints `ready`, and then
/// runs `answers` (a shell `case` over `$line`) once per request line.
fn fake_interpreter(name: &str, ready: &str, answers: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/oracle-fake");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    let path = dir.join(name);
    let script = format!(
        "#!/bin/sh\nn=0\nprintf '%s\\n' '{ready}'\nwhile IFS= read -r line; do\n{answers}\ndone\n"
    );
    std::fs::write(&path, script).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
    path.to_str().expect("the path is text").to_string()
}

/// The fake 1.0 checker: `slow` times out, `bad` errs, everything else is equivalent.
fn fake_checker(name: &str) -> String {
    fake_interpreter(
        name,
        r#"{"ready": true}"#,
        r#"  case "$line" in
    *'"expected":"slow"'*) printf '%s\n' '{"timeout": true}' ;;
    *'"expected":"bad"'*) printf '%s\n' '{"error": "no reading"}' ;;
    *) printf '%s\n' '{"equivalent": true, "notation": false, "timeout": false}' ;;
  esac"#,
    )
}

#[test]
fn the_verdict_reader_keeps_a_decided_verdict_and_a_timeout_in_order() {
    let python = fake_checker("check.sh");
    let pairs = [pair("7329", "7.329"), pair("slow", "x"), pair("6", "6.0")];
    let decided = Some(OracleVerdict {
        equivalent: true,
        notation: false,
    });
    assert_eq!(live_verdicts(&python, &pairs), vec![decided, None, decided]);
}

#[test]
#[should_panic(expected = "the oracle said {\"error\": \"no reading\"}")]
fn the_verdict_reader_refuses_an_oracle_error() {
    let python = fake_checker("check_error.sh");
    live_verdicts(&python, &[pair("bad", "1")]);
}

#[test]
fn one_live_response_returns_the_trimmed_line() {
    let python = fake_checker("check_one.sh");
    let request = serde_json::json!({"expected": "slow", "learner": "x", "kind": "numeric"});
    assert_eq!(
        one_live_response(&python, &request, "0.05"),
        r#"{"timeout": true}"#
    );
}

#[test]
#[should_panic(expected = "the oracle said \"{\\\"boom\\\": 1}\\n\"")]
fn a_helper_that_does_not_say_ready_fails_the_test() {
    let python = fake_interpreter("check_boom.sh", r#"{"boom": 1}"#, "  :");
    one_live_response(&python, &serde_json::json!({}), "0.05");
}

/// The fake rewrite helper. It refuses every second answer, and it spells the
/// other answers twice: once verbatim and once as `(answer)*1` under `rule`.
fn fake_rewriter(name: &str, rule: &str) -> String {
    fake_interpreter(
        name,
        r#"{"ready": true}"#,
        &format!(
            r#"  case "$line" in
    *'"op":"rewrite"'*)
      n=$((n+1))
      if [ $((n % 2)) -eq 0 ]; then printf '%s\n' '{{"error": "refused"}}'; continue; fi
      answer=$(printf '%s' "$line" | sed 's/^{{"answer":"\(.*\)","op":"rewrite"}}$/\1/')
      printf '%s\n' "{{\"spellings\": [{{\"rule\": \"cancel\", \"text\": \"$answer\"}}, {{\"rule\": \"{rule}\", \"text\": \"($answer)*1\"}}]}}"
      ;;
    *'"op":"difference"'*) printf '%s\n' '{{"cancel_zero": true, "radsimp_zero": false}}' ;;
  esac"#
        ),
    )
}

#[test]
fn the_rewrite_reader_keeps_one_spelling_per_offered_row_and_sorts_them() {
    let python = fake_rewriter("rewrite.sh", "expand");
    let rows = live_rewrites(&python);
    let offered = in_grammar_rows()
        .iter()
        .filter(|row| {
            holds_a_denominator_or_a_radical(&row.ast)
                && the_two_checkers_read_the_answer_alike(row)
        })
        .count();
    assert!(offered > 100, "the corpus offers {offered} rows");
    assert_eq!(
        rows.len(),
        offered - offered / 2,
        "every second row is refused"
    );
    for row in &rows {
        assert_eq!(row.rule, "expand", "{}", row.answer);
        assert_eq!(row.learner, format!("({})*1", row.answer));
        assert!(row.cancel_zero && !row.radsimp_zero, "{}", row.answer);
    }
    let mut sorted = rows.clone();
    sorted.sort_by(|left, right| {
        (&left.answer, &left.kind, &left.rule).cmp(&(&right.answer, &right.kind, &right.rule))
    });
    assert!(rows == sorted, "the rows come out sorted");
}

#[test]
#[should_panic(expected = "the helper wrote the unknown rule \"bogus\"")]
fn the_rewrite_reader_refuses_an_unknown_rule() {
    let python = fake_rewriter("rewrite_bogus.sh", "bogus");
    live_rewrites(&python);
}
