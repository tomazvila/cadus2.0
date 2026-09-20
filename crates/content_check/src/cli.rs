//! The argument frame: options, the request of `grade` and `mutants`, the
//! batch file, and the loop that turns each request into one document. This
//! file holds no business logic.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::output::{Reply, error_doc};

/// The usage line of the error document.
pub const USAGE: &str = "usage: content_check <grade|mutants|report|row|diff|dump-kp|selftest> \
[options] [--pretty]";

/// One request of `grade` or `mutants`: a command line, or one batch line.
#[derive(Debug, Deserialize)]
pub struct Request {
    /// The contract JSON. The subcommand does the typed parse.
    pub contract: Value,
    pub expected: String,
    #[serde(default)]
    pub learner: Option<String>,
}

/// The work of one subcommand: a request gives a document or an error text.
pub type Work = fn(&Request) -> Result<Value, String>;

/// The requests of one command.
enum Input {
    One(Request),
    /// For each batch line: the `id` and the request, or why the line is bad.
    Batch(Vec<(Value, Result<Request, String>)>),
}

/// Remove each occurrence of a flag with no value. True if the flag was there.
pub fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|arg| arg != flag);
    args.len() != before
}

/// Read `--name value` pairs. A value can start with `-` (a negative key).
fn options(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>, String> {
    let mut found = BTreeMap::new();
    let mut rest = args.iter();
    while let Some(name) = rest.next() {
        let key = name
            .strip_prefix("--")
            .filter(|key| allowed.contains(key))
            .ok_or_else(|| format!("unknown option `{name}`"))?;
        let value = rest
            .next()
            .ok_or_else(|| format!("the option `{name}` needs a value"))?;
        if found.insert(key.to_owned(), value.clone()).is_some() {
            return Err(format!("the option `{name}` occurs two times"));
        }
    }
    Ok(found)
}

/// Take one option that the command cannot run without.
fn necessary(found: &mut BTreeMap<String, String>, key: &str) -> Result<String, String> {
    found
        .remove(key)
        .ok_or_else(|| format!("the option `--{key}` is necessary"))
}

fn input(args: &[String], allowed: &[&str]) -> Result<Input, String> {
    let mut found = options(args, allowed)?;
    if let Some(path) = found.remove("batch") {
        if !found.is_empty() {
            return Err("`--batch` takes no other option".to_owned());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read `{path}`: {error}"))?;
        let lines: Vec<_> = text.lines().map(batch_line).collect();
        if lines.is_empty() {
            return Err(format!("the batch file `{path}` has no line"));
        }
        return Ok(Input::Batch(lines));
    }
    let contract = necessary(&mut found, "contract")?;
    let contract = serde_json::from_str(&contract)
        .map_err(|error| format!("the contract is not JSON: {error}"))?;
    let expected = necessary(&mut found, "expected")?;
    let learner = found.remove("learner");
    Ok(Input::One(Request {
        contract,
        expected,
        learner,
    }))
}

/// Read one batch line: `{"id","contract","expected","learner"}`.
fn batch_line(line: &str) -> (Value, Result<Request, String>) {
    match serde_json::from_str::<Value>(line) {
        Ok(doc) => {
            let id = doc.get("id").cloned().unwrap_or(Value::Null);
            let request = serde_json::from_value(doc)
                .map_err(|error| format!("the batch line is not a request: {error}"));
            (id, request)
        }
        Err(error) => (
            Value::Null,
            Err(format!("the batch line is not JSON: {error}")),
        ),
    }
}

/// Run `work` for the one request, or for each batch line in the file order.
///
/// A bad request gives the error document and exit 2. In a batch, the error
/// document takes the place of that line only, and each document has its `id`.
pub fn run_each(args: &[String], allowed: &[&str], work: Work) -> Reply {
    match input(args, allowed) {
        Err(text) => Reply::error(&text, 2),
        Ok(Input::One(request)) => match work(&request) {
            Ok(doc) => Reply::one(doc, 0),
            Err(text) => Reply::error(&text, 2),
        },
        Ok(Input::Batch(lines)) => {
            let mut exit = 0;
            let docs = lines
                .into_iter()
                .map(|(id, request)| {
                    let mut doc =
                        request
                            .and_then(|request| work(&request))
                            .unwrap_or_else(|text| {
                                exit = 2;
                                error_doc(&text, 2)
                            });
                    doc["id"] = id;
                    doc
                })
                .collect();
            Reply { docs, exit }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn take_flag_removes_the_flag_and_reports_it() {
        let mut args = strings(&["grade", "--pretty", "--expected", "1"]);
        assert!(take_flag(&mut args, "--pretty"));
        assert_eq!(args, strings(&["grade", "--expected", "1"]));
        assert!(!take_flag(&mut args, "--pretty"));
    }

    #[test]
    fn options_accept_a_value_that_starts_with_a_minus() {
        let found = options(&strings(&["--expected", "-3/(3x+1)^2"]), &["expected"]);
        assert_eq!(
            found.ok().and_then(|mut found| found.remove("expected")),
            Some("-3/(3x+1)^2".to_owned())
        );
    }

    #[test]
    fn options_refuse_an_unknown_name_and_a_name_with_no_value() {
        let allowed = ["expected"];
        assert_eq!(
            options(&strings(&["--learner", "1"]), &allowed).err(),
            Some("unknown option `--learner`".to_owned())
        );
        assert_eq!(
            options(&strings(&["expected", "1"]), &allowed).err(),
            Some("unknown option `expected`".to_owned())
        );
        assert_eq!(
            options(&strings(&["--expected", "1", "--expected", "2"]), &allowed).err(),
            Some("the option `--expected` occurs two times".to_owned())
        );
        assert_eq!(
            options(&strings(&["--expected"]), &allowed).err(),
            Some("the option `--expected` needs a value".to_owned())
        );
    }
}
