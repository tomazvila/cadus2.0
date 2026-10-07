//! The live quality check of the proof grader (Amendment K point 6).
//!
//! Ignored by default: it calls the real hosted model with the real prompt,
//! over 18 fixture proofs of 6 standard statements (one correct proof, one
//! correct but unusually written proof, and one with a planted error each),
//! and prints one row per proof. It asserts nothing about the model: the
//! table is the result.
//!
//! Run it with the worker's model environment (`OPENAI_API_KEY`,
//! `OPENAI_BASE_URL`, `OPENROUTER_PROVIDER_ORDER`, optional
//! `PROOF_GRADER_MODEL`):
//!
//! ```text
//! cargo test -p cadus-worker --test proof_grading_live -- --ignored --nocapture
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use std::sync::Arc;

use cadus_model_client::{Client, ModelConfig};
use cadus_store::proof_grading::JobPayload;
use cadus_worker::proof_grading::{
    CALL_TIMEOUT, DEFAULT_MODEL, DEFAULT_OUTPUT_TOKENS, DEFAULT_REASONING_MAX_TOKENS, grade,
};
use serde_json::Value;

/// How many calls run at once.
const PARALLEL: usize = 6;

fn client() -> Client {
    let mut cfg = ModelConfig::from_env().expect("the model environment reads");
    cfg.model = std::env::var("PROOF_GRADER_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    cfg.output_tokens = DEFAULT_OUTPUT_TOKENS;
    cfg.reasoning_max_tokens = DEFAULT_REASONING_MAX_TOKENS;
    cfg.timeout = CALL_TIMEOUT;
    Client::new(cfg).expect("the client builds")
}

#[tokio::test]
#[ignore = "calls the real hosted model; run by hand with the model environment"]
async fn the_grader_sorts_the_fixture_proofs() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/proof_grading_quality.json")).unwrap();
    let client = Arc::new(client());
    println!("model: {}", client.config().model);
    let mut cases = Vec::new();
    for statement in fixture["statements"].as_array().unwrap() {
        for proof in statement["proofs"].as_array().unwrap() {
            let payload = JobPayload {
                v: 1,
                task_id: "live".to_owned(),
                topic: statement["id"].as_str().unwrap().to_owned(),
                item_digest: "0123456789ab".to_owned(),
                problem: statement["problem"].as_str().unwrap().to_owned(),
                reference: statement["reference"].as_str().map(str::to_owned),
                expected: None,
                rubric: Vec::new(),
                given_answer: proof["text"].as_str().unwrap().to_owned(),
                kp: None,
                mode: None,
                problem_hash: None,
            };
            cases.push((
                proof["id"].as_str().unwrap().to_owned(),
                proof["valid"].as_bool().unwrap(),
                payload,
            ));
        }
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    let mut set = tokio::task::JoinSet::new();
    for (index, (id, valid, payload)) in cases.into_iter().enumerate() {
        let client = Arc::clone(&client);
        let semaphore = Arc::clone(&semaphore);
        set.spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            let started = std::time::Instant::now();
            let (attempts, result) = grade(&client, &payload).await;
            (
                index,
                id,
                valid,
                attempts,
                result,
                started.elapsed().as_secs(),
            )
        });
    }
    let mut rows = Vec::new();
    while let Some(joined) = set.join_next().await {
        rows.push(joined.unwrap());
    }
    rows.sort_by_key(|row| row.0);

    let (mut wrong_passed, mut valid_failed, mut errors) = (0, 0, 0);
    println!("| proof | expected | verdict | unmet checks | secs | http |");
    println!("|---|---|---|---|---|---|");
    for (_, id, valid, attempts, result, secs) in &rows {
        let expected = if *valid { "valid" } else { "planted error" };
        match result {
            Ok(grading) => {
                let unmet: Vec<String> = grading
                    .checks
                    .iter()
                    .filter(|c| !c.met)
                    .map(|c| format!("{}{}", c.id, if c.minor { "(minor)" } else { "" }))
                    .collect();
                if grading.passed() && !valid {
                    wrong_passed += 1;
                }
                if !grading.passed() && *valid {
                    valid_failed += 1;
                }
                println!(
                    "| {id} | {expected} | {} | {} | {secs} | {} |",
                    grading.verdict,
                    unmet.join(" "),
                    attempts.len()
                );
            }
            Err(reason) => {
                errors += 1;
                println!(
                    "| {id} | {expected} | ERROR: {reason} | | {secs} | {} |",
                    attempts.len()
                );
            }
        }
    }
    println!(
        "\nwrong proofs passed: {wrong_passed}; valid proofs failed: {valid_failed}; errors: {errors}"
    );
    for (_, id, _, _, result, _) in &rows {
        if let Ok(grading) = result {
            println!("\n## {id} ({})\n{}", grading.verdict, grading.feedback);
            for check in &grading.checks {
                println!(
                    "- {} [{}{}] {} — {}",
                    check.id,
                    if check.met { "yes" } else { "no" },
                    if check.minor { ", minor" } else { "" },
                    check.text,
                    check.evidence
                );
            }
        }
    }
}
