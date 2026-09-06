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
    // `sqrt(x)` is the root atom of `x` since the rational-exponent production
    // of D-F3 (unit f2-grammar).
    let sqrt_x = r#"root(Poly({{Var("x"): 1}: Ratio { numer: 1, denom: 1 }}), 2)**1"#;
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
    // The rational-exponent production of D-F3 reads `x^(1/2)` as the root of
    // `x`, so the walk reaches the variable under the root atom.
    assert!(holds_the_variable_x("x^(1/2)"));
    assert!(
        !holds_the_variable_x("x^(1/7)"),
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
/// The fake interpreters, written once under `target/oracle-fake`.
///
/// One thread writes every script before any test starts a child process, so no
/// write handle is open across a fork, and no exec meets a busy text file.
fn fakes() -> &'static Path {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/oracle-fake");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
        write_fake(&dir.join("check.sh"), r#"{"ready": true}"#, CHECKER);
        write_fake(&dir.join("boom.sh"), r#"{"boom": 1}"#, "  :");
        write_fake(
            &dir.join("rewrite.sh"),
            r#"{"ready": true}"#,
            &rewriter("expand"),
        );
        write_fake(
            &dir.join("rewrite_bogus.sh"),
            r#"{"ready": true}"#,
            &rewriter("bogus"),
        );
        dir
    })
}

/// The path of the fake interpreter `name`, as text.
fn fake(name: &str) -> String {
    fakes()
        .join(name)
        .to_str()
        .expect("the path is text")
        .to_string()
}

/// Write one fake interpreter at `path`.
///
/// The script ignores the helper path and the timeout, prints `ready`, and then
/// runs `answers` (a shell `case` over `$line`) once per request line.
fn write_fake(path: &Path, ready: &str, answers: &str) {
    let script = format!(
        "#!/bin/sh\nn=0\nprintf '%s\\n' '{ready}'\nwhile IFS= read -r line; do\n{answers}\ndone\n"
    );
    std::fs::write(path, script).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
}

/// The fake 1.0 checker: `slow` times out, `bad` errs, everything else is equivalent.
const CHECKER: &str = r#"  case "$line" in
    *'"expected":"slow"'*) printf '%s\n' '{"timeout": true}' ;;
    *'"expected":"bad"'*) printf '%s\n' '{"error": "no reading"}' ;;
    *) printf '%s\n' '{"equivalent": true, "notation": false, "timeout": false}' ;;
  esac"#;

/// The fake rewrite helper. It refuses every second answer, and it spells the
/// other answers twice: once verbatim and once as `(answer)*1` under `rule`.
fn rewriter(rule: &str) -> String {
    format!(
        r#"  case "$line" in
    *'"op":"rewrite"'*)
      n=$((n+1))
      if [ $((n % 2)) -eq 0 ]; then printf '%s\n' '{{"error": "refused"}}'; continue; fi
      answer=$(printf '%s' "$line" | sed 's/^{{"answer":"\(.*\)","op":"rewrite"}}$/\1/')
      printf '%s\n' "{{\"spellings\": [{{\"rule\": \"cancel\", \"text\": \"$answer\"}}, {{\"rule\": \"{rule}\", \"text\": \"($answer)*1\"}}]}}"
      ;;
    *'"op":"difference"'*) printf '%s\n' '{{"cancel_zero": true, "radsimp_zero": false}}' ;;
  esac"#
    )
}

#[test]
fn the_verdict_reader_keeps_a_decided_verdict_and_a_timeout_in_order() {
    let python = fake("check.sh");
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
    let python = fake("check.sh");
    live_verdicts(&python, &[pair("bad", "1")]);
}

#[test]
fn one_live_response_returns_the_trimmed_line() {
    let python = fake("check.sh");
    let request = serde_json::json!({"expected": "slow", "learner": "x", "kind": "numeric"});
    assert_eq!(
        one_live_response(&python, &request, "0.05"),
        r#"{"timeout": true}"#
    );
}

#[test]
#[should_panic(expected = "the oracle said \"{\\\"boom\\\": 1}\\n\"")]
fn a_helper_that_does_not_say_ready_fails_the_test() {
    let python = fake("boom.sh");
    one_live_response(&python, &serde_json::json!({}), "0.05");
}

#[test]
fn the_rewrite_reader_keeps_one_spelling_per_offered_row_and_sorts_them() {
    let python = fake("rewrite.sh");
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
    let python = fake("rewrite_bogus.sh");
    live_rewrites(&python);
}
