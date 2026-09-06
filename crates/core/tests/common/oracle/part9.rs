//! Part 9 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Start one oracle helper of `scripts/oracle`, and hand back its process and pipes.
pub fn start_harness(
    python: &str,
    script_name: &str,
    timeout: &str,
) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/oracle")
        .join(script_name)
        .canonicalize()
        .unwrap_or_else(|e| panic!("find scripts/oracle/{script_name}: {e}"));
    let mut child = Command::new(python)
        .arg(&script)
        .arg("--timeout")
        .arg(timeout)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("start {}: {e}", script.display()));
    let stdin = child.stdin.take().unwrap_or_else(|| panic!("no stdin"));
    let stdout = child.stdout.take().unwrap_or_else(|| panic!("no stdout"));
    (child, stdin, BufReader::new(stdout))
}

/// Read the ready line of a helper, and fail the test when it says anything else.
pub fn expect_ready(reader: &mut BufReader<ChildStdout>, who: &str) {
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .unwrap_or_else(|e| panic!("read the ready line: {e}"));
    assert!(ready.contains("\"ready\""), "{who} said {ready:?}");
}

// ---------------------------------------------------------------------------
// The live oracle
// ---------------------------------------------------------------------------
/// Ask the live rewrite helper for the spellings of every row that qualifies.
///
/// The helper is `scripts/oracle/rewrite_1_0.py`. It runs one request at a time,
/// and the caller reads one response before it writes the next one, so neither
/// pipe ever fills.
pub fn live_rewrites(python: &str) -> Vec<RewriteLine> {
    let (mut child, mut stdin, mut reader) = start_harness(python, "rewrite_1_0.py", "10.0");
    expect_ready(&mut reader, "the helper");

    let mut ask = |request: &serde_json::Value| -> serde_json::Value {
        writeln!(stdin, "{request}").unwrap_or_else(|e| panic!("write the request: {e}"));
        stdin.flush().unwrap_or_else(|e| panic!("flush: {e}"));
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .unwrap_or_else(|e| panic!("read the response: {e}"));
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("response {line}: {e}"))
    };

    let mut out: Vec<RewriteLine> = Vec::new();
    let mut offered = 0_usize;
    let mut refused_by_1_0 = 0_usize;
    for row in in_grammar_rows() {
        if !holds_a_denominator_or_a_radical(&row.ast)
            || !the_two_checkers_read_the_answer_alike(&row)
        {
            continue;
        }
        offered += 1;
        let response = ask(&serde_json::json!({
            "op": "rewrite",
            "answer": row.answer,
        }));
        // 1.0 refuses some corpus answers itself (`\frac{1}{2}` reaches SymPy as
        // `frac{1}{2}`). The helper reports the error, and the row carries no
        // spelling. That is a 1.0 defect, and other tests pin it.
        if response.get("error").is_some() {
            refused_by_1_0 += 1;
            continue;
        }
        let spellings = response
            .get("spellings")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        for spelling in spellings {
            let rule = spelling
                .get("rule")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let learner = spelling
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !REWRITE_RULES.contains(&rule.as_str()) {
                panic!("the helper wrote the unknown rule {rule:?}");
            }
            // A spelling that repeats the answer or the printed tree is no
            // variant, and it would only repeat a pair another family builds.
            if learner == row.answer || learner == row.printed {
                continue;
            }
            let difference = ask(&serde_json::json!({
                "op": "difference",
                "expected": row.answer,
                "learner": learner,
            }));
            let flag = |name: &str| {
                difference
                    .get(name)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            };
            out.push(RewriteLine {
                answer: row.answer.clone(),
                kind: row.kind.as_str().to_string(),
                rule,
                learner,
                cancel_zero: flag("cancel_zero"),
                radsimp_zero: flag("radsimp_zero"),
            });
        }
    }
    drop(stdin);
    let _ = child.wait();
    println!(
        "rewrite: {offered} rows offered, {refused_by_1_0} refused by the 1.0 parser, \
         {} spellings kept",
        out.len()
    );
    out.sort_by(|left, right| {
        (&left.answer, &left.kind, &left.rule).cmp(&(&right.answer, &right.kind, &right.rule))
    });
    out
}

/// Ask the live 1.0 checker for every verdict of the generated set.
pub fn live_verdicts(python: &str, pairs: &[Pair]) -> Vec<Option<OracleVerdict>> {
    let (mut child, mut stdin, mut reader) = start_harness(python, "check_1_0.py", "2.0");
    let requests: Vec<String> = pairs
        .iter()
        .map(|pair| {
            serde_json::json!({
                "expected": pair.expected,
                "learner": pair.learner,
                "kind": pair.kind.as_str(),
            })
            .to_string()
        })
        .collect();
    let writer = std::thread::spawn(move || {
        for line in requests {
            if writeln!(stdin, "{line}").is_err() {
                return;
            }
        }
        drop(stdin);
    });
    expect_ready(&mut reader, "the oracle");
    let mut out = Vec::with_capacity(pairs.len());
    for line in reader.lines() {
        let line = line.unwrap_or_else(|e| panic!("read a verdict: {e}"));
        let row: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("verdict {line}: {e}"));
        assert!(row.get("error").is_none(), "the oracle said {line}");
        if row.get("timeout").and_then(serde_json::Value::as_bool) == Some(true) {
            out.push(None);
        } else {
            out.push(Some(OracleVerdict {
                equivalent: row
                    .get("equivalent")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| panic!("verdict {line} has no `equivalent`")),
                notation: row
                    .get("notation")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or_else(|| panic!("verdict {line} has no `notation`")),
            }));
        }
    }
    let _ = writer.join();
    let _ = child.wait();
    out
}

/// The committed line of one pair and its 1.0 verdict. A timeout carries null verdict fields.
pub fn verdict_line(pair: &Pair, verdict: Option<OracleVerdict>) -> serde_json::Value {
    let (equivalent, notation) = match verdict {
        Some(verdict) => (
            serde_json::Value::Bool(verdict.equivalent),
            serde_json::Value::Bool(verdict.notation),
        ),
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    serde_json::json!({
        "expected": pair.expected,
        "learner": pair.learner,
        "kind": pair.kind.as_str(),
        "equivalent": equivalent,
        "notation": notation,
        "timeout": verdict.is_none(),
    })
}

/// Ask the live 1.0 oracle for one pair, and return the raw response line.
///
/// The helper starts one harness process, sends one request, and reads one
/// response. It exists for the guard test below, which needs its own guard.
pub fn one_live_response(python: &str, request: &serde_json::Value, timeout_s: &str) -> String {
    let (mut child, mut stdin, mut reader) = start_harness(python, "check_1_0.py", timeout_s);
    writeln!(stdin, "{request}").unwrap_or_else(|e| panic!("write the request: {e}"));
    drop(stdin);
    expect_ready(&mut reader, "the oracle");
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .unwrap_or_else(|e| panic!("read the response: {e}"));
    let _ = child.wait();
    response.trim().to_string()
}

// ---------------------------------------------------------------------------
// The residue: the corpus answers the grammar refuses (V2)
// ---------------------------------------------------------------------------
/// One corpus line, with the fields the residue report needs.
#[derive(serde::Deserialize)]
pub struct ResidueLine {
    pub answer: String,
    pub answer_kind: String,
    pub shape: String,
    pub topic_id: String,
    pub kp_id: String,
    pub exemplar_index: i64,
}

// ---------------------------------------------------------------------------
// The token-soup fuzz (V3)
// ---------------------------------------------------------------------------
/// Every token the corpus answers use, in sorted order.
///
/// A token is one run of alphanumeric characters or one other character. The
/// soup therefore reaches deeper into the grammar than a random byte string
/// does: it holds real function names, real Unicode glyphs, and real brackets.
pub fn corpus_vocabulary() -> Vec<String> {
    let path = fixture("corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let row: CorpusLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let mut run = String::new();
        for ch in row.answer.chars() {
            if ch.is_alphanumeric() {
                run.push(ch);
                continue;
            }
            if !run.is_empty() {
                tokens.insert(std::mem::take(&mut run));
            }
            tokens.insert(ch.to_string());
        }
        if !run.is_empty() {
            tokens.insert(run);
        }
    }
    tokens.into_iter().collect()
}

/// Build one random answer from the corpus vocabulary.
pub fn token_soup(rng: &mut Rng, vocabulary: &[String]) -> String {
    let length = 1 + (rng.next() % 24) as usize;
    let mut out = String::new();
    for _ in 0..length {
        let index = (rng.next() as usize) % vocabulary.len();
        out.push_str(vocabulary.get(index).map_or("0", String::as_str));
        if rng.next().is_multiple_of(4) {
            out.push(' ');
        }
    }
    out
}

/// Print a tuple, a set, or a list between its delimiters.
pub fn print_wrapped(ast: &Ast, items: &[Ast]) -> String {
    let (open, close) = match ast {
        Ast::Tuple(_) => ('(', ')'),
        Ast::Set(_) => ('{', '}'),
        _ => ('[', ']'),
    };
    format!("{open}{}{close}", print_list(items))
}
