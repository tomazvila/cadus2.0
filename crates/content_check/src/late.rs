//! The subcommands of lane B5b: `report`, `row`, `diff`, `dump-kp`, `selftest`.
//!
//! This file holds the entry `run`, the option reader and the error type of
//! the five subcommands. Each subcommand has its own module.

#[path = "db.rs"]
pub mod db;
#[path = "diff.rs"]
mod diff;
#[path = "dump_kp.rs"]
mod dump_kp;
#[path = "invariants/mod.rs"]
pub mod invariants;
#[path = "kp_view.rs"]
pub mod kp_view;
#[path = "report.rs"]
mod report;
#[path = "row.rs"]
mod row;
#[path = "selftest.rs"]
mod selftest;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::output::error_doc;

/// An error that stops a subcommand: the text and the exit code (2 or 3).
#[derive(Debug, PartialEq, Eq)]
pub struct Fail {
    pub text: String,
    pub exit: u8,
}

impl Fail {
    /// A usage or input error (exit 2).
    pub fn input(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit: 2,
        }
    }

    /// The loader refused the tree (exit 3, invariant I1).
    pub fn refused(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit: 3,
        }
    }
}

/// The document and the exit code of one subcommand.
pub type Outcome = Result<(Value, u8), Fail>;

/// The options of one command line: `--name value` pairs and bare flags.
#[derive(Debug, Default)]
pub struct Opts {
    values: BTreeMap<String, String>,
}

impl Opts {
    /// Read the arguments. `valued` names take a value; `flags` names do not.
    pub fn read(args: &[String], valued: &[&str], flags: &[&str]) -> Result<Self, Fail> {
        let mut opts = Self::default();
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            let name = arg.strip_prefix("--").unwrap_or("");
            if flags.contains(&name) {
                opts.values.insert(name.to_owned(), String::new());
            } else if valued.contains(&name) {
                let value = rest
                    .next()
                    .ok_or_else(|| Fail::input(format!("the option `{arg}` needs a value")))?;
                opts.values.insert(name.to_owned(), value.clone());
            } else {
                return Err(Fail::input(format!("unknown option `{arg}`")));
            }
        }
        Ok(opts)
    }

    /// The value of an option, if the command line has it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// True if the command line has the option or the flag.
    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// The value of an option that the subcommand cannot run without.
    pub fn need(&self, name: &str) -> Result<&str, Fail> {
        self.get(name)
            .ok_or_else(|| Fail::input(format!("the option `--{name}` is necessary")))
    }
}

/// Read a JSON file. A file that is absent or is not JSON is an input error.
pub fn read_json(path: &str) -> Result<Value, Fail> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| Fail::input(format!("cannot read `{path}`: {error}")))?;
    serde_json::from_str(&text)
        .map_err(|error| Fail::input(format!("`{path}` is not JSON: {error}")))
}

/// Run one late subcommand. `None` tells `main.rs` that the subcommand is not
/// in this build.
pub fn run(subcommand: &str, args: &[String]) -> Option<(Value, u8)> {
    let outcome = match subcommand {
        "dump-kp" => dump_kp::run(args),
        "report" => report::run(args),
        "row" => row::run(args),
        "diff" => diff::run(args),
        "selftest" => selftest::run(args),
        _ => return None,
    };
    Some(outcome.unwrap_or_else(|fail| (error_doc(&fail.text, fail.exit), fail.exit)))
}

#[cfg(test)]
mod tests {
    use super::{Fail, Opts, run};

    /// The arguments of one command line, from one text with spaces.
    fn strings(line: &[&str]) -> Vec<String> {
        line.iter().copied().map(String::from).collect()
    }

    #[test]
    fn opts_read_values_and_flags() {
        let opts = Opts::read(&strings(&["--all", "--base", "dir"]), &["base"], &["all"]);
        let opts = opts.unwrap_or_default();
        assert!(opts.has("all"));
        assert_eq!(opts.get("base"), Some("dir"));
        assert_eq!(opts.need("base"), Ok("dir"));
        assert_eq!(
            opts.need("kp"),
            Err(Fail::input("the option `--kp` is necessary"))
        );
    }

    #[test]
    fn opts_refuse_an_unknown_name_and_a_name_with_no_value() {
        let unknown = Opts::read(&strings(&["--bad"]), &["base"], &[]);
        assert_eq!(unknown.err(), Some(Fail::input("unknown option `--bad`")));
        let bare = Opts::read(&strings(&["base"]), &["base"], &[]);
        assert_eq!(bare.err(), Some(Fail::input("unknown option `base`")));
        let short = Opts::read(&strings(&["--base"]), &["base"], &[]);
        assert_eq!(
            short.err(),
            Some(Fail::input("the option `--base` needs a value"))
        );
    }

    #[test]
    fn an_unknown_subcommand_is_not_in_this_build() {
        assert_eq!(run("other", &[]), None);
    }
}
