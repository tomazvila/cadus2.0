//! Shared exhaustive assertions for checked-in pending-template batches.

use std::collections::BTreeSet;

use super::{json_rows, repo_root};
use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
    template::{Compiled, from_body, walk_satisfying},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum AnswerPolicy {
    Unique,
    LabelOrUnique,
    Adversarial,
}

pub fn read_rows(paths: &[&str]) -> Vec<Value> {
    json_rows(paths, None)
}

pub fn assert_exhaustive_batch(
    rows: Vec<Value>,
    expected_rows: usize,
    expected_problems: usize,
    tuple_count: fn(&str) -> usize,
    answer_policy: AnswerPolicy,
) {
    let (curriculum, findings) = load_curriculum(&repo_root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut problems = BTreeSet::new();
    assert_eq!(rows.len(), expected_rows);
    for row in rows {
        assert_eq!(row["status"], "pending");
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|error| panic!("{key}: {error:?}"));
        let doc = from_body(&body).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
        let count = tuple_count(key);
        assert_eq!(walk.tuples.len(), count, "{key}");
        assert!(walk.exhaustive);
        assert_eq!(doc.samples.len(), count);
        let mut answers = BTreeSet::new();
        for binding in walk.tuples {
            let sample = doc
                .samples
                .iter()
                .find(|item| item.bindings() == binding)
                .unwrap();
            let instance = compiled.instantiate(binding).unwrap();
            let result = check_contract(
                &sample.expected.text(),
                &instance.answer,
                doc.answer_contract.clone().unwrap(),
            );
            assert!(matches!(result, Outcome::Decided(verdict) if verdict.correct));
            assert!(problems.insert(instance.text));
            if matches!(answer_policy, AnswerPolicy::Adversarial) {
                assert_wrong_contract_answers(
                    &instance.answer,
                    doc.answer_contract.clone().unwrap(),
                    key,
                );
            }
            answers.insert(instance.answer);
        }
        assert_answers(key, &row, answers, count, answer_policy);
    }
    assert_eq!(problems.len(), expected_problems);
}

fn assert_answers(
    key: &str,
    row: &Value,
    answers: BTreeSet<String>,
    count: usize,
    policy: AnswerPolicy,
) {
    if matches!(policy, AnswerPolicy::LabelOrUnique)
        && row["arguments"]["answer_contract"]["kind"] == "label"
    {
        assert_eq!(answers, BTreeSet::from(["yes".to_owned(), "no".to_owned()]));
    } else if !matches!(policy, AnswerPolicy::Adversarial) {
        assert_eq!(answers.len(), count, "{key}: repeated or collapsed answer");
    }
}

fn assert_wrong_contract_answers(
    answer: &str,
    contract: cadus_core::answer::AnswerContract,
    key: &str,
) {
    let perturbed = if answer.contains('(') {
        answer.replacen('(', "(1+", 1)
    } else if answer.contains("yes") {
        answer.replacen("yes", "no", 1)
    } else {
        answer.replacen("no", "yes", 1)
    };
    assert!(
        matches!(check_contract(answer, &perturbed, contract.clone()),
            Outcome::Decided(verdict) if !verdict.correct),
        "{key}: {perturbed}"
    );
    for wrong in ["999", "(999,999)", "yes", "no", "none", "all points"] {
        assert!(
            !matches!(check_contract(answer, wrong, contract.clone()),
                Outcome::Decided(verdict) if verdict.correct),
            "{key}: {wrong}"
        );
    }
}

pub fn assert_rejected_expressions(rows: Vec<Value>, expressions: &[&str], bad_sample: &str) {
    let (curriculum, _) = load_curriculum(&repo_root().join("curriculum")).unwrap();
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        for expression in expressions {
            let mut arguments = row["arguments"].clone();
            arguments["answer_expr"] = json!(expression);
            assert!(
                verify_kind(Kind::Template, &spec, &arguments, &[]).is_err(),
                "{key}: {expression}"
            );
        }
        let mut arguments = row["arguments"].clone();
        arguments["samples"][0]["expected"] = json!(bad_sample);
        assert!(verify_kind(Kind::Template, &spec, &arguments, &[]).is_err());
    }
}

#[macro_export]
macro_rules! template_batch_tests {
    ($paths:expr, $rows:expr, $problems:expr, $count:expr, $policy:expr, $expressions:expr, $bad:expr) => {
        #[test]
        fn all_pending_instances_pass_the_real_worker_and_match_exhaustive_samples() {
            $crate::common::template_batch::assert_exhaustive_batch(
                $crate::common::template_batch::read_rows($paths),
                $rows,
                $problems,
                $count,
                $policy,
            );
        }

        #[test]
        fn worker_rejects_degenerate_expressions_and_false_samples() {
            $crate::common::template_batch::assert_rejected_expressions(
                $crate::common::template_batch::read_rows($paths),
                $expressions,
                $bad,
            );
        }
    };
}
