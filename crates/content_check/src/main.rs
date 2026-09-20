//! `content_check`: the judge of the content campaign.
//!
//! The command prints one JSON document on stdout for each result and sends
//! diagnostics to stderr. The contract of the command line is the file
//! `content-check-cli.md` of the freeze pack.

mod cli;
mod grade;
mod late;
mod mutants;
mod output;
// Lane B4b adds `mutant_for` to this shared file. This crate does not call it.
#[allow(dead_code)]
#[path = "../../check_keys/src/mutate.rs"]
mod mutate;

use std::process::ExitCode;

use output::Reply;

/// The subcommands that `late::run` owns.
const LATE: [&str; 5] = ["report", "row", "diff", "dump-kp", "selftest"];

fn main() -> ExitCode {
    let args: Result<Vec<String>, _> = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect();
    let Ok(mut args) = args else {
        return output::emit(&Reply::error("an argument is not valid UTF-8", 2), false);
    };
    let pretty = cli::take_flag(&mut args, "--pretty");
    output::emit(&dispatch(&args), pretty)
}

/// Send the arguments to the subcommand that the first argument names.
fn dispatch(args: &[String]) -> Reply {
    let Some((subcommand, rest)) = args.split_first() else {
        return Reply::error(cli::USAGE, 2);
    };
    match subcommand.as_str() {
        "grade" => grade::run(rest),
        "mutants" => mutants::run(rest),
        name if LATE.contains(&name) => {
            let (doc, exit) = late::run(name, rest).unwrap_or_else(|| {
                let text = format!("the subcommand `{name}` is not in this build");
                (output::error_doc(&text, 4), 4)
            });
            Reply::one(doc, exit)
        }
        name => Reply::error(&format!("unknown subcommand `{name}`; {}", cli::USAGE), 2),
    }
}
