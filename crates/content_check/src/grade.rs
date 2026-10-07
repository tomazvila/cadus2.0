//! `grade`: one learner text against one key under one contract.
//!
//! The verdict comes from `cadus_core::answer::check_contract` and from
//! nothing else.

use cadus_core::answer::{AnswerContract, Outcome, check_contract, format_hint};
use serde_json::{Value, json};

use crate::cli::{self, Request};
use crate::output::Reply;

/// Run `grade` with the arguments that follow the subcommand.
pub fn run(args: &[String]) -> Reply {
    cli::run_each(
        args,
        &["contract", "expected", "learner", "batch"],
        grade_doc,
    )
}

/// The typed contract of a contract JSON. The error text goes to exit 2.
pub fn parse_contract(contract: &Value) -> Result<AnswerContract, String> {
    serde_json::from_value(contract.clone())
        .map_err(|error| format!("the contract does not deserialize: {error}"))
}

/// The `verdict`, `reason` and `notation` values of one outcome.
pub fn fields(outcome: Outcome) -> (&'static str, Option<&'static str>, bool) {
    match outcome {
        Outcome::Decided(verdict) if verdict.correct => ("correct", None, verdict.notation),
        Outcome::Decided(verdict) => ("wrong", None, verdict.notation),
        Outcome::Undecidable(refusal) => ("ungraded", Some(refusal.reason), false),
    }
}

fn grade_doc(request: &Request) -> Result<Value, String> {
    let learner = request
        .learner
        .as_deref()
        .ok_or("`grade` needs a learner text (`--learner`, or `learner` in the batch line)")?;
    let contract = parse_contract(&request.contract)?;
    let (verdict, reason, notation) =
        fields(check_contract(&request.expected, learner, contract.clone()));
    // A wrong answer with a notation hint (a mixed number written as a product,
    // or in another form) carries the hint in `reason` and `notation: true`.
    let hint = (verdict == "wrong")
        .then(|| format_hint(&request.expected, learner, &contract))
        .flatten();
    let notation = notation || hint.is_some();
    let reason = hint.map(Value::from).unwrap_or_else(|| json!(reason));
    Ok(
        json!({"schema": "cadus.grade.v1", "verdict": verdict, "reason": reason,
        "notation": notation}),
    )
}
