//! The command line, read against the manual: its commands, the manual's
//! pages, and what docfit exits with.

use agent_help::Help;
use clap::{FromArgMatches, Parser, Subcommand};
use serde_json::{Value, json};

/// The command finished and found nothing wrong.
pub(crate) const EXIT_OK: i32 = 0;
/// The command could not finish: a bad command line, or a repo it cannot read.
pub(crate) const EXIT_ERROR: i32 = 2;

/// Keeps a repo's documents each in its one home, within its budget.
///
/// A CLI for AI coding agents, beside slimfit (code size and tests) and truefit (ties to the real system). Run `docfit <command> --help` before a command's first run.
///
/// ## Output
///
/// Every command prints one JSON object on stdout. A command that cannot finish exits {const:EXIT_ERROR} with `{"error": [{"fault": "...", "fix": "..."}, ...]}`: every fault it found, each with the command or edit that fixes it.
#[derive(Parser)]
#[command(
    name = "docfit",
    version = env!("CARGO_PKG_VERSION"),
    verbatim_doc_comment,
    disable_help_subcommand = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Shows one topic of this manual, or all of it.
    ///
    /// `docfit help all` shows the whole manual.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Help {
        /// The topic, or `all`.
        #[arg(verbatim_doc_comment)]
        topic: Option<String>,
    },
}

pub(crate) fn help() -> Help {
    Help::new(<Cli as clap::CommandFactory>::command()).constant("EXIT_ERROR", EXIT_ERROR)
}

/// The command line's answer and exit code. `--help`, `--version` and a
/// command line clap refuses print and exit here, as clap does.
pub(crate) fn run(args: impl IntoIterator<Item = std::ffi::OsString>) -> (String, i32) {
    let help = help();
    let command = help
        .clap()
        .expect("the manual is whole: checked by its test");
    let cli = match command
        .try_get_matches_from(args)
        .and_then(|m| Cli::from_arg_matches(&m))
    {
        Ok(cli) => cli,
        Err(e) if !e.use_stderr() => e.exit(),
        Err(e) => return refusal(&e.to_string(), "run `docfit --help`"),
    };
    match cli.command {
        Cmd::Help { topic: None } => (help.index().expect("checked"), EXIT_OK),
        Cmd::Help { topic: Some(topic) } => match help.topic_help(&topic) {
            Ok(markdown) => (markdown, EXIT_OK),
            Err(e) => refusal(&e.to_string(), "run `docfit --help` for the topics"),
        },
    }
}

/// A command that could not finish: its one fault and fix, exit {EXIT_ERROR}.
pub(crate) fn refusal(fault: &str, fix: &str) -> (String, i32) {
    (
        pretty(&json!({ "error": [{ "fault": fault.trim(), "fix": fix }] })),
        EXIT_ERROR,
    )
}

pub(crate) fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("json")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manual_is_whole() {
        agent_help::check(&super::help()).unwrap();
    }
}
