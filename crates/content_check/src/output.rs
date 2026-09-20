//! The output types: the finding, the error document, the JSON printer and the
//! exit code. This file holds no business logic.

use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Value, json};

/// One finding of a content check, in the frozen form of `content-check-cli.md`.
///
/// `invariant`, `item` and `hash` are `null` in the JSON when they do not apply.
// Lane B5b builds the findings; lane B5a has no subcommand that reports one.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub ck: String,
    pub code: String,
    pub invariant: Option<String>,
    pub kp: String,
    pub item: Option<String>,
    pub hash: Option<String>,
    pub detail: String,
}

/// The documents and the exit code of one command.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    /// One document for a single request; one document for each batch line.
    pub docs: Vec<Value>,
    pub exit: u8,
}

impl Reply {
    /// A reply with one document.
    pub fn one(doc: Value, exit: u8) -> Self {
        Self {
            docs: vec![doc],
            exit,
        }
    }

    /// A reply that holds the `cadus.error.v1` document.
    pub fn error(text: &str, exit: u8) -> Self {
        Self::one(error_doc(text, exit), exit)
    }
}

/// The `cadus.error.v1` document of the exit codes 2, 3 and 4.
pub fn error_doc(text: &str, exit: u8) -> Value {
    json!({"schema": "cadus.error.v1", "error": text, "exit": exit})
}

/// The stdout text of a reply: one line for each document unless `pretty`.
pub fn render(reply: &Reply, pretty: bool) -> String {
    reply
        .docs
        .iter()
        .map(|doc| {
            if pretty {
                format!("{doc:#}\n")
            } else {
                format!("{doc}\n")
            }
        })
        .collect()
}

/// Print the reply and give the exit code to the caller.
///
/// Each error document also goes to stderr as one diagnostic line.
pub fn emit(reply: &Reply, pretty: bool) -> ExitCode {
    print!("{}", render(reply, pretty));
    for doc in &reply.docs {
        if let Some(text) = doc.get("error").and_then(Value::as_str) {
            eprintln!("content_check: {text}");
        }
    }
    ExitCode::from(reply.exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_has_the_frozen_keys_and_null_for_absent_values() {
        let finding = Finding {
            ck: "CK7".to_owned(),
            code: "duplicate".to_owned(),
            invariant: Some("I5".to_owned()),
            kp: "course/topic/kp".to_owned(),
            item: None,
            hash: None,
            detail: "text".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&finding).ok(),
            Some(json!({"ck": "CK7", "code": "duplicate", "invariant": "I5",
                "kp": "course/topic/kp", "item": null, "hash": null, "detail": "text"}))
        );
    }

    #[test]
    fn render_writes_one_line_for_each_document() {
        let reply = Reply {
            docs: vec![json!({"a": 1}), json!({"b": 2})],
            exit: 0,
        };
        assert_eq!(render(&reply, false), "{\"a\":1}\n{\"b\":2}\n");
        assert_eq!(
            render(&Reply::one(json!({"a": 1}), 0), true),
            "{\n  \"a\": 1\n}\n"
        );
    }

    #[test]
    fn error_reply_holds_the_error_document() {
        let reply = Reply::error("bad", 2);
        assert_eq!(reply.exit, 2);
        assert_eq!(
            reply.docs,
            vec![json!({"schema": "cadus.error.v1", "error": "bad", "exit": 2})]
        );
    }
}
