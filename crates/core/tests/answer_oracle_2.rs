//! Part 2 of the `answer_oracle` tests. The header of `answer_oracle_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::oracle::*;

#[test]
fn the_generated_set_is_deterministic_and_capped() {
    let first = generated_pairs();
    let second = generated_pairs();
    assert_eq!(first.len(), second.len(), "the pair count moved");
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left.expected, right.expected);
        assert_eq!(left.learner, right.learner);
        assert_eq!(left.generator, right.generator);
    }
    assert!(
        first.len() <= PAIR_CAP,
        "the generated set holds {} pairs, and the cap is {PAIR_CAP}",
        first.len()
    );
    assert_eq!(first.len(), GENERATED_PAIRS, "the generated pair count");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for pair in &first {
        *counts.entry(pair.generator).or_insert(0) += 1;
    }
    for (name, want) in GENERATOR_COUNTS {
        let found = counts.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    for name in counts.keys() {
        assert!(
            GENERATOR_COUNTS.iter().any(|(known, _)| known == name),
            "{name} is a generator the count table does not list"
        );
    }
}

#[test]
fn dump_the_generated_pairs_when_asked() {
    let Ok(path) = std::env::var("CADUS_ORACLE_DUMP") else {
        return;
    };
    let mut out = String::new();
    for pair in generated_pairs() {
        let line = serde_json::json!({
            "expected": pair.expected,
            "learner": pair.learner,
            "kind": pair.kind.as_str(),
            "generator": pair.generator,
            "intent": pair.intent.name(),
        });
        let _ = writeln!(out, "{line}");
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote the generated pairs to {path}");
}

#[test]
fn the_two_checkers_agree_on_every_comparable_pair() {
    let pairs = generated_pairs();
    let verdicts = committed_verdicts();
    let report = build_report(&pairs, &verdicts);
    print_report(&report);
    for (name, want) in CLASS_COUNTS {
        let found = report.per_class.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    for (name, want) in REASON_COUNTS {
        let found = report.per_reason.get(name).copied().unwrap_or(0);
        assert_eq!(found, want, "{name}: pair count");
    }
    assert!(
        report.disagreements.is_empty(),
        "R5: {} class 3 pairs disagree with the 1.0 oracle:\n{}",
        report.disagreements.len(),
        report.disagreements.join("\n")
    );
    assert_eq!(
        report.comparable_agreed, report.comparable,
        "class 3 agreement must be 100%"
    );
}

#[test]
fn the_1_0_float_rung_class_splits_into_a_rounding_and_a_residue() {
    // The unit asks for the literal counts, so the test walks the same 240 pairs
    // and reads the 2.0 outcome of each one. `check` runs here; the split is
    // measured, and the three numbers are pinned above.
    let pairs = generated_pairs();
    let verdicts = committed_verdicts();
    let (mut rounded, mut wrong, mut undecidable) = (0_usize, 0_usize, 0_usize);
    for pair in &pairs {
        if pair.shape == "prose_or_words" {
            continue;
        }
        let key = (
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        );
        let Some(Some(oracle)) = verdicts.get(&key).copied() else {
            continue;
        };
        // The class is what `documented_reason` named before the rule: 1.0 says
        // the two answers are equal, and the 1.0 float rung is the reason.
        if !oracle.equivalent || !the_1_0_float_rung_closes_the_gap(pair) {
            continue;
        }
        match check(&pair.expected, &pair.learner, pair.kind) {
            // The pair is the same value on both sides, so it never diverged and
            // it was never in the class. The float rung closes a gap of zero.
            Outcome::Decided(verdict) if verdict.correct && !verdict.notation => {}
            // The rounding rule refuses it; every other refusal was already
            // there, and its pair is class 1.
            Outcome::Undecidable(refusal) => {
                if ROUNDING_REFUSALS.contains(&refusal.reason) {
                    undecidable += 1;
                }
            }
            Outcome::Decided(verdict) if verdict.correct => rounded += 1,
            Outcome::Decided(_) => wrong += 1,
        }
    }
    assert_eq!((rounded, wrong, undecidable), D6_SPLIT);
    assert_eq!(rounded + wrong + undecidable, 240);
}

#[test]
fn every_documented_reason_is_one_of_the_named_sixteen() {
    for (name, _) in REASON_COUNTS {
        assert!(
            DOCUMENTED_REASONS.contains(&name),
            "{name} is not a documented divergence"
        );
    }
    assert_eq!(DOCUMENTED_REASONS.len(), REASON_COUNTS.len());
}

#[test]
fn the_committed_verdict_file_covers_the_generated_set_exactly() {
    let pairs = generated_pairs();
    let verdicts = committed_verdicts();
    let mut wanted: BTreeSet<PairKey> = BTreeSet::new();
    for pair in &pairs {
        wanted.insert((
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        ));
    }
    let recorded: BTreeSet<PairKey> = verdicts.keys().cloned().collect();
    let missing: Vec<&PairKey> = wanted.difference(&recorded).take(10).collect();
    let extra: Vec<&PairKey> = recorded.difference(&wanted).take(10).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the recorded set moved: missing {missing:?}, extra {extra:?}"
    );
    assert_eq!(recorded.len(), wanted.len());
}

/// Write `rational_rewrites_1_0.jsonl` from the live helper, when asked.
///
/// The step runs once, by hand, and it commits its result:
///
/// ```text
/// CADUS_REWRITE_REGEN=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
///     cargo test -p cadus-core --test answer_oracle regenerate_the_rewrite_fixture
/// ```
#[test]
fn regenerate_the_rewrite_fixture_when_asked() {
    if std::env::var("CADUS_REWRITE_REGEN").is_err() {
        return;
    }
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        panic!("set CADUS_ORACLE_PYTHON to regenerate the rewrite fixture");
    };
    let rows = live_rewrites(&python);
    let mut out = String::new();
    for row in &rows {
        let line = serde_json::to_string(row).unwrap_or_else(|e| panic!("write a row: {e}"));
        let _ = writeln!(out, "{line}");
    }
    let path = fixture("rational_rewrites_1_0.jsonl");
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "wrote {} rewrite spellings to {}",
        rows.len(),
        path.display()
    );
}

/// Prove the committed spellings still say what the live helper says.
#[test]
fn the_live_rewrite_helper_reproduces_the_committed_spellings() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run the live rewrite helper");
        return;
    };
    if std::env::var("CADUS_REWRITE_REGEN").is_ok() {
        println!("skipped: the regeneration test owns the file in this run");
        return;
    }
    let live = live_rewrites(&python);
    let committed = committed_rewrites();
    assert_eq!(
        live.len(),
        committed.len(),
        "the live helper wrote {} spellings and the file holds {}",
        live.len(),
        committed.len()
    );
    let mut moved = Vec::new();
    for (live, recorded) in live.iter().zip(committed.iter()) {
        if live != recorded {
            moved.push(format!(
                "{:?} {}: recorded {:?}, live {:?}",
                recorded.answer, recorded.rule, recorded.learner, live.learner
            ));
        }
    }
    assert!(
        moved.is_empty(),
        "the live rewrite helper no longer matches the committed file:\n{}",
        moved.join("\n")
    );
    println!(
        "the live rewrite helper reproduced {} spellings",
        live.len()
    );
}

/// Write `oracle_verdicts_1_0.jsonl` from the live 1.0 checker, when asked.
///
/// The step runs once, by hand, after a generator change, and it commits its
/// result:
///
/// ```text
/// CADUS_ORACLE_RECORD=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
///     cargo test -p cadus-core --test answer_oracle record_the_oracle_verdicts
/// ```
#[test]
fn record_the_oracle_verdicts_when_asked() {
    if std::env::var("CADUS_ORACLE_RECORD").is_err() {
        return;
    }
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        panic!("set CADUS_ORACLE_PYTHON to record the 1.0 verdicts");
    };
    let pairs = generated_pairs();
    let live = live_verdicts(&python, &pairs);
    assert_eq!(live.len(), pairs.len(), "the oracle answered every pair");
    let mut out = String::new();
    for (pair, verdict) in pairs.iter().zip(live.iter()) {
        let _ = writeln!(out, "{}", verdict_line(pair, *verdict));
    }
    let path = fixture("oracle_verdicts_1_0.jsonl");
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "recorded {} 1.0 verdicts in {}",
        pairs.len(),
        path.display()
    );
}

/// Prove the committed file still says what the live 1.0 checker says.
///
/// The test needs the 1.0 interpreter, which only this build box has, so it runs
/// when `CADUS_ORACLE_PYTHON` names that interpreter and it skips otherwise. The
/// gate therefore always compares against the committed file, and a person who
/// has 1.0 checks the file itself with one environment variable.
#[test]
fn the_live_oracle_reproduces_the_committed_verdicts() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run against the live 1.0 checker");
        return;
    };
    if std::env::var("CADUS_ORACLE_RECORD").is_ok() {
        println!("skipped: the record test owns the file in this run");
        return;
    }
    let pairs = generated_pairs();
    let live = live_verdicts(&python, &pairs);
    assert_eq!(live.len(), pairs.len(), "the oracle answered every pair");
    let committed = committed_verdicts();
    let mut moved = Vec::new();
    for (pair, live) in pairs.iter().zip(live.iter()) {
        let key = (
            pair.expected.clone(),
            pair.learner.clone(),
            pair.kind.as_str().to_string(),
        );
        let recorded = committed.get(&key).copied().unwrap_or_else(|| {
            panic!("the committed file has no verdict for {key:?}");
        });
        if recorded != *live {
            moved.push(format!("{key:?}: recorded {recorded:?}, live {live:?}"));
        }
    }
    assert!(
        moved.is_empty(),
        "the live 1.0 checker no longer matches the committed file:\n{}",
        moved.join("\n")
    );
    println!("the live 1.0 checker reproduced {} verdicts", pairs.len());
}

/// A pair the guard stops is recorded as a timeout, never as a decided verdict.
///
/// Review finding #21: the old guard raised a `Timeout` exception inside the 1.0
/// process, and the bare `except Exception` handlers of 1.0 `_sympy_equivalent`
/// caught it and returned `False`. The harness now runs every 1.0 call in a
/// child process and terminates that process on the deadline, so 1.0 cannot
/// swallow the guard. The pair below is the work bomb of spec section 3.2.
#[test]
fn a_stopped_oracle_call_is_a_timeout_and_never_a_decided_verdict() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        println!("skipped: set CADUS_ORACLE_PYTHON to run against the live 1.0 checker");
        return;
    };
    let request = serde_json::json!({
        "expected": "(x+1)**200",
        "learner": "x**200+1",
        "kind": "expression",
        "id": "stall",
    });
    let response = one_live_response(&python, &request, "0.05");
    println!("stall response: {response}");
    assert_eq!(
        response, r#"{"equivalent": null, "id": "stall", "notation": null, "timeout": true}"#,
        "the guard must report a timeout, and it must decide nothing"
    );
    // The worker respawns, so the next pair still gets a real 1.0 verdict.
    let next = serde_json::json!({
        "expected": "7329",
        "learner": "7.329",
        "kind": "numeric",
        "id": "after",
    });
    let after = one_live_response(&python, &next, "0.05");
    println!("after response: {after}");
    assert_eq!(
        after, r#"{"equivalent": true, "id": "after", "notation": true, "timeout": false}"#,
        "a fast pair keeps its decided 1.0 verdict"
    );
}

/// Write the residue of `docs/reference/undecidable-answers.md`, when asked.
///
/// The dump carries the shape bucket, the topic, and the refusal reason of the
/// 2.0 grammar, so the owner and M6 authoring read one file per group.
#[test]
fn dump_the_undecidable_residue_when_asked() {
    let Ok(path) = std::env::var("CADUS_RESIDUE_DUMP") else {
        return;
    };
    let corpus = fixture("corpus_1_0.jsonl");
    let text = std::fs::read_to_string(&corpus)
        .unwrap_or_else(|e| panic!("read {}: {e}", corpus.display()));
    let mut out = String::new();
    let mut count = 0_usize;
    for line in text.lines() {
        let row: ResidueLine =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
        let source = normalize(&row.answer).source;
        let Err(refusal) = parse(&source) else {
            continue;
        };
        count += 1;
        let record = serde_json::json!({
            "answer": row.answer,
            "answer_kind": row.answer_kind,
            "shape": row.shape,
            "topic_id": row.topic_id,
            "kp_id": row.kp_id,
            "exemplar_index": row.exemplar_index,
            "source": source,
            "reason": refusal.reason,
        });
        let _ = writeln!(out, "{record}");
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {count} refused answers to {path}");
}

#[test]
fn ten_seconds_of_corpus_token_soup_never_panics() {
    let vocabulary = corpus_vocabulary();
    assert!(
        vocabulary.len() > 200,
        "the corpus vocabulary holds {} tokens",
        vocabulary.len()
    );
    let mut rng = Rng(0x2026_0826_7f13_0091);
    let cases = checker_never_panics(&mut rng, |rng| token_soup(rng, &vocabulary));
    assert!(cases > 1_000, "the soup fuzz ran only {cases} cases");
    println!("the token-soup fuzz ran {cases} cases");
}
