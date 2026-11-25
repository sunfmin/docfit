//! The command line, read against the manual: its commands, the manual's
//! pages, and what docfit exits with.

use crate::budget::{self, BASELINE, Baseline, Measured};
use crate::documents::{self, Answer, CONFIG, Refusal, TABLE};
use crate::kind::Kind;
use crate::repo;
use agent_help::Help;
use clap::{FromArgMatches, Parser, Subcommand};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The command finished and found nothing wrong.
pub(crate) const EXIT_OK: i32 = 0;
/// The command finished and found faults, each with its fix.
pub(crate) const EXIT_FAULTS: i32 = 1;
/// The command could not finish: a bad command line, or a repo it cannot read.
pub(crate) const EXIT_ERROR: i32 = 2;

/// Keeps a repo's documents each in its one home, within its budget.
///
/// A CLI for AI coding agents, beside slimfit (code size and tests) and truefit (ties to the real system). Run `docfit <command> --help` before a command's first run.
///
/// A repo declares its documents in `docfit.toml`, at its top: each Markdown file git tracks is registered by a glob under one kind, and a kind says who reads the file and what it holds, nothing else. The kinds are fixed in docfit:
///
/// {const:KINDS}
///
/// Promises live in ADRs: there is no spec kind.
///
/// ## Budgets
///
/// Each document has a budget: its size in bytes, and its longest line in characters. The table above gives each kind's default; a `docfit.toml` entry may set its own (see `docfit check --help`). `docfit check` is red on a document over its budget, unless the baseline, `{const:BASELINE}`, holds it at more: then it is red only once it grows past what the baseline holds. `docfit ratchet` moves the baseline down to the documents' sizes now, and never up. So a repo adopting docfit with a document far over its budget runs `docfit ratchet` once and is green that day, and every change after can only shrink it.
///
/// ## Output
///
/// Every command prints one JSON object on stdout with a `next` field: what to do next. A command that finds faults exits {const:EXIT_FAULTS}; each fault carries its `fix`. Nothing wrong exits {const:EXIT_OK}. A command that cannot finish exits {const:EXIT_ERROR} with `{"error": [{"fault": "...", "fix": "..."}, ...]}`: every fault it found, each with the command or edit that fixes it. Do every fix, then run the command again.
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
    /// Checks that every Markdown file git tracks is registered in `docfit.toml` under one kind, within its budget.
    ///
    /// ## When to run it
    ///
    /// In the repo's gate, and after adding, moving, renaming or growing a Markdown file. It reads only git's index, `docfit.toml`, the baseline and the documents, so it takes well under a second.
    ///
    /// ## What it reads and writes
    ///
    /// It reads `docfit.toml` at the top of the repo, and the paths git tracks there (`git ls-files`): every one ending `.md` is a document. A file on disk that git does not track is not. It reads each registered document's text from the working tree, and the baseline `{const:BASELINE}` if there is one. It writes nothing. `docfit.toml` holds one table, `[documents]`, a line per glob, the glob a path from the repo's top and the kind one of the kinds in `docfit --help`, or a table that also sets the document's budget, `bytes` (its size) or `line` (its longest line, in characters) or both:
    ///
    /// ```toml
    /// [documents]
    /// "README.md" = { kind = "readme", bytes = 6144 }
    /// "CLAUDE.md" = "agent"
    /// "docs/agents/*.md" = "agent"
    /// "docs/adr/*.md" = { kind = "adr", line = 120 }
    /// ```
    ///
    /// In a glob, `*` and `?` stay within one directory and `**` crosses directories, as in a `.gitignore`. A measure no matching entry sets is the kind's default; where several set it, the smallest holds.
    ///
    /// The baseline maps a document to the measures it is held at over its budget, as `docfit ratchet` writes it: `{"README.md": {"bytes": 65536, "line": 312}}`. A document within its budget is held at its budget.
    ///
    /// ## What it answers
    ///
    /// - `documents`: each registered document's path, with its kind.
    /// - `faults`: in path order, each file that is not registered under exactly one kind, and each document over what it may be. Empty when there is none. Each has `file`, `fault` and `fix`:
    ///   - `unregistered`: no glob matches the file. `kinds` are the kinds it could be: the one its place says, such as `agent` for `CLAUDE.md` or a file in `docs/agents/`, otherwise every kind; `fix` is the line to add.
    ///   - `two_kinds`: globs of several kinds match it; `kinds` are those kinds.
    ///   - `over_budget`: its size in bytes, `size`, is over `limit`, its budget or, when the baseline holds it at more, that.
    ///   - `line_too_long`: its line `line` is `size` characters, over `limit`, as above. The fix is to break it at its sentences, one per line; `docfit fmt` (docfit#8) will do it once it exists.
    /// - `next`: the first fault's fix; with none, `docfit ratchet` when a document shrank below what the baseline holds, or that there is nothing to do.
    ///
    /// For example, with `CLAUDE.md` registered and `notes.md` not:
    ///
    /// ```json
    /// {
    ///   "documents": { "CLAUDE.md": "agent" },
    ///   "faults": [
    ///     { "file": "notes.md", "fault": "unregistered", "kinds": ["readme", "agent", "adr", "glossary", "design-note"], "fix": "add `\"notes.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note" }
    ///   ],
    ///   "next": "add `\"notes.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note"
    /// }
    /// ```
    ///
    /// It exits {const:EXIT_FAULTS} when there is a fault, {const:EXIT_OK} when there is none.
    ///
    /// ## What it may refuse
    ///
    /// It exits {const:EXIT_ERROR}, naming every fault at once, outside a git repository, without a `docfit.toml` or its `[documents]` table, on a `docfit.toml` that is not TOML, on a glob that is not one, on an entry that is neither a kind nor a table of `kind`, `bytes` and `line`, on a kind docfit does not know (the fix lists the kinds), on a document it cannot read as UTF-8 text, and on a baseline that is not one.
    ///
    /// ## What trips you up
    ///
    /// - A design note is off unless the repo turns it on: registering a file as `design-note` is turning it on, for that file.
    /// - A glob matches paths from the repo's top, whatever directory you run it in.
    /// - It measures the working tree, not git's index: an edit not yet staged counts.
    /// - A document that shrank stays held at the old size until `docfit ratchet` runs: run it and commit the baseline with the change.
    #[command(
        next_help_heading = "Run by the agent, and by the gate",
        verbatim_doc_comment
    )]
    Check {
        /// The repository to check (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
    /// Moves the baseline down to the documents' sizes now; it never moves it up.
    ///
    /// ## When to run it
    ///
    /// Once, when a repo adopts docfit with documents over their budgets: it holds each at its size now, so `docfit check` is green that day. After that, with a change that shrinks a document held over its budget, when `docfit check`'s `next` says so; commit `{const:BASELINE}` with the change.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads, and writes `{const:BASELINE}` (whole, through a file beside it renamed over it) when the baseline moves. With no baseline yet, it holds each document over its budget, in each measure it is over, at its size now. With one, each measure held moves to the smaller of what is held and the size now; a measure it does not hold stays unheld, so a document that grows over its budget is never added; a measure back within budget leaves it.
    ///
    /// ## What it answers
    ///
    /// - `changed`: whether the baseline moved. `false` is nothing to ratchet, and it writes nothing.
    /// - `baseline`: the baseline now, each held document with its held `bytes` and `line`.
    /// - `next`: commit the baseline, or that there is nothing to ratchet.
    ///
    /// It exits {const:EXIT_OK}.
    ///
    /// ## What it may refuse
    ///
    /// What `docfit check` refuses, and a baseline it cannot write; it exits {const:EXIT_ERROR} and writes nothing. A file not registered is not refused: it is not a document, so not held.
    ///
    /// ## What trips you up
    ///
    /// - Deleting the baseline and running it again holds every document at its size now: that is adopting docfit again, and shows in the diff.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Ratchet {
        /// The repository to ratchet (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
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
    Help::new(<Cli as clap::CommandFactory>::command())
        .constant("KINDS", Kind::table())
        .constant("BASELINE", BASELINE)
        .constant("EXIT_OK", EXIT_OK)
        .constant("EXIT_FAULTS", EXIT_FAULTS)
        .constant("EXIT_ERROR", EXIT_ERROR)
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
        Err(e) => return refused(vec![refusal(e.to_string().trim(), "run `docfit --help`")]),
    };
    match cli.command {
        Cmd::Check { repo } => check(repo).unwrap_or_else(refused),
        Cmd::Ratchet { repo } => ratchet(repo).unwrap_or_else(refused),
        Cmd::Help { topic: None } => (help.index().expect("checked"), EXIT_OK),
        Cmd::Help { topic: Some(topic) } => match help.topic_help(&topic) {
            Ok(markdown) => (markdown, EXIT_OK),
            Err(e) => refused(vec![refusal(
                e.to_string().trim(),
                "run `docfit --help` for the topics",
            )]),
        },
    }
}

/// The repo's top, and its documents registered.
fn registered(repo: Option<PathBuf>) -> Result<(PathBuf, Answer), Vec<Refusal>> {
    let top = repo::top(&repo.unwrap_or_else(|| PathBuf::from(".")))?;
    let config = std::fs::read_to_string(top.join(CONFIG)).map_err(|e| {
        vec![refusal(
            &format!("cannot read {CONFIG} at the repo's top: {e}"),
            &format!("write {CONFIG} with a `[{TABLE}]` table, a line `\"<glob>\" = \"<kind>\"` per document (see `docfit check --help`)"),
        )]
    })?;
    let entries = documents::entries(&config)?;
    let answer = documents::check(&entries, &repo::tracked(&top)?);
    Ok((top, answer))
}

/// Each registered document's text, read from the working tree.
fn texts(top: &Path, answer: &Answer) -> Result<Vec<String>, Vec<Refusal>> {
    let (mut texts, mut refusals) = (Vec::new(), Vec::new());
    for file in answer.documents.keys() {
        match std::fs::read_to_string(top.join(file)) {
            Ok(text) => texts.push(text),
            Err(e) => refusals.push(refusal(
                &format!("cannot read `{file}`, which git tracks: {e}"),
                &format!("restore `{file}` as UTF-8 text, or `git rm` it"),
            )),
        }
    }
    if refusals.is_empty() {
        Ok(texts)
    } else {
        Err(refusals)
    }
}

fn measured<'a>(answer: &'a Answer, texts: &'a [String]) -> Vec<Measured<'a>> {
    answer
        .documents
        .iter()
        .zip(texts)
        .map(|((file, doc), text)| Measured {
            file,
            budget: doc.budget,
            text,
        })
        .collect()
}

/// The baseline at the repo's top: `None` when it has none yet.
fn baseline(top: &Path) -> Result<Option<Baseline>, Vec<Refusal>> {
    match std::fs::read_to_string(top.join(BASELINE)) {
        Ok(text) => budget::baseline(Some(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => budget::baseline(None),
        Err(e) => Err(vec![refusal(
            &format!("cannot read {BASELINE}: {e}"),
            &format!("make {BASELINE} readable, or restore it from git"),
        )]),
    }
}

fn check(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let (top, answer) = registered(repo)?;
    let texts = texts(&top, &answer)?;
    let docs = measured(&answer, &texts);
    let held = baseline(&top)?;
    let mut faults = answer.faults.clone();
    faults.extend(budget::check(
        &docs,
        held.as_ref().unwrap_or(&Baseline::new()),
        held.is_none(),
    ));
    faults.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
    let tighter = held
        .as_ref()
        .is_some_and(|h| &budget::ratchet(&docs, Some(h)) != h);
    let (code, next) = match faults.first() {
        Some(first) => (EXIT_FAULTS, first["fix"].clone()),
        None if tighter => (
            EXIT_OK,
            json!(format!(
                "run `docfit ratchet`: a document shrank, and {BASELINE} can hold it tighter"
            )),
        ),
        None => (
            EXIT_OK,
            json!(
                "nothing to do: every Markdown file is registered under one kind, within its budget"
            ),
        ),
    };
    let kinds: std::collections::BTreeMap<_, _> = answer
        .documents
        .iter()
        .map(|(f, d)| (f, d.kind.name()))
        .collect();
    Ok((
        pretty(&json!({ "documents": kinds, "faults": faults, "next": next })),
        code,
    ))
}

fn ratchet(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let (top, answer) = registered(repo)?;
    let texts = texts(&top, &answer)?;
    let held = baseline(&top)?;
    let next = budget::ratchet(&measured(&answer, &texts), held.as_ref());
    let changed = held.as_ref() != Some(&next);
    if changed {
        write(&top.join(BASELINE), &(pretty(&json!(next)) + "\n"))?;
    }
    let then = if changed {
        format!("commit {BASELINE} with the change, then `docfit check`")
    } else {
        "nothing to ratchet: run `docfit check`".to_string()
    };
    Ok((
        pretty(&json!({ "changed": changed, "baseline": next, "next": then })),
        EXIT_OK,
    ))
}

/// `text` written to `path` whole or not at all: a temp file beside it, renamed over it.
fn write(path: &Path, text: &str) -> Result<(), Vec<Refusal>> {
    let failed = |e: std::io::Error| {
        vec![refusal(
            &format!("cannot write {}: {e}", path.display()),
            &format!("make {} writable, then run it again", path.display()),
        )]
    };
    let dir = path.parent().expect("the baseline is in a directory");
    std::fs::create_dir_all(dir).map_err(failed)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(failed)?;
    std::fs::rename(&temp, path).map_err(failed)
}

pub(crate) fn refusal(fault: &str, fix: &str) -> Refusal {
    Refusal {
        fault: fault.to_string(),
        fix: fix.to_string(),
    }
}

/// A command that could not finish: every fault and its fix, exit {EXIT_ERROR}.
fn refused(refusals: Vec<Refusal>) -> (String, i32) {
    (pretty(&json!({ "error": refusals })), EXIT_ERROR)
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("json")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manual_is_whole() {
        agent_help::check(&super::help()).unwrap();
        <super::Cli as clap::CommandFactory>::command().debug_assert();
        let manual = super::help().manual().unwrap();
        assert!(!manual.contains("{const:"), "{manual}");
    }
}
