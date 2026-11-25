//! The command line, read against the manual: its commands, the manual's
//! pages, and what docfit exits with.

use crate::budget::{self, BASELINE, Baseline, Measured, Size, Stray};
use crate::documents::{self, Answer, CONFIG, Document, Refusal, TABLE};
use crate::kind::Kind;
use crate::pointers::{self, Tree};
use crate::repo;
use crate::sentences;
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
/// Each document has a budget: its size in bytes, and its longest line in characters. The table above gives each kind's default; a `docfit.toml` entry may set its own (see `docfit check --help`). `docfit check` is red on a document over its budget, unless the baseline, `{const:BASELINE}`, holds it at more: then it is red only once it grows past what the baseline holds. `docfit ratchet` moves the baseline down to the documents' sizes now, and never up. A Markdown file no kind fits (there is no spec kind) may be held **to rehome**: `docfit check` is green on it while it does not grow, and red once it does; `docfit ratchet` only shrinks what it is held at, and drops it once the file is gone or registered under a kind. A pointer broken when a repo adopts docfit is held too, by its document and the pointer: `docfit check` is green on it, and red on a pointer broken since; `docfit ratchet` drops it once it is fixed, and never adds one. So a repo adopting docfit with a document far over its budget, a broken pointer, or a file no kind fits, runs `docfit init` (or, with its `docfit.toml` written, `docfit ratchet`) once and is green that day, and every change after can only shrink it.
///
/// ## Pointers
///
/// What a document points at stays true: each repo path it names in backticks or links to, and each ADR it names by number (`ADR 0007` is a file `{const:ADRS}0007-*`), must be a path git tracks, or a directory holding one. `docfit check` is red on one that is not, naming the document, the line and what is missing. A pointer that breaks shows at once; a copy that goes stale shows nowhere, so point rather than copy.
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
    /// Checks that every Markdown file git tracks is registered in `docfit.toml` under one kind, within its budget, one sentence per line, and that what it points at is there.
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
    /// The baseline maps a document to the measures it is held at over its budget and the pointers it is held at broken, and a file registered under no kind to the bytes it is held at to rehome, as `docfit ratchet` writes it: `{"README.md": {"bytes": 65536, "line": 312, "pointers": ["docs/old.md"]}, "SPEC.md": {"rehome": 15360}}`. A document within its budget is held at its budget. A pointer is held by what it is written as, its `pointer`: the same pointer written again in that document is held too.
    ///
    /// ## What it answers
    ///
    /// - `documents`: each registered document's path, with its kind.
    /// - `faults`: in path order, each file that is not registered under exactly one kind, and each document over what it may be. Empty when there is none. Each has `file`, `fault` and `fix`:
    ///   - `unregistered`: no glob matches the file. `kinds` are the kinds it could be: the one its place says, such as `agent` for `CLAUDE.md` or a file in `docs/agents/`, otherwise every kind; `fix` is the line to add.
    ///   - `two_kinds`: globs of several kinds match it; `kinds` are those kinds.
    ///   - `rehome_grew`: a file registered under no kind, held to rehome, is `size` bytes, over `limit`, the bytes the baseline holds it at. Held to rehome, a file registered under no kind is no `unregistered` fault.
    ///   - `over_budget`: its size in bytes, `size`, is over `limit`, its budget or, when the baseline holds it at more, that.
    ///   - `line_too_long`: its line `line` is `size` characters, over `limit`, as above. The fix is to break it at its sentences, one per line, which `docfit fmt` does; a sentence still too long is cut by hand.
    ///   - `unformatted`: `docfit fmt` would change it, first at its line `line`: some line holds more than one sentence. The fix is `docfit fmt <file>`.
    ///   - `broken_pointer`: its line `line` points, as `pointer`, at `missing`, which git does not track, and the baseline does not hold that pointer broken in that document. A pointer is:
    ///     - a backticked repo path: with a `/` in it, or a name ending in one of {const:EXTENSIONS}, and none of a command's space, a glob's `*`, a placeholder's `<>`, a URL's `:` (but a `:<line>` after it), or a leading `/` or `~`. A name with a `/` and none of those extensions is a pointer only when its first segment (`src` of `src/a/b`) is something git tracks at the repo's top or in the document's directory; otherwise it is a name, such as a model id `org/model` or a git ref `origin/main`, and is not checked. A pointer is read from the repo's top or the document's directory, and is there if either is.
    ///     - a Markdown link's target, `[text](target)`, read from the document's directory (a leading `/` is the repo's top), without its `#anchor`. A URL (`https:`, `mailto:`) and an anchor alone are not checked.
    ///     - an ADR named by number, `ADR 0007` or `ADR-0007`: there must be a file `{const:ADRS}0007-*`.
    ///
    ///     A fenced code block points at nothing.
    /// - `next`: the first fault's fix; with none, `docfit ratchet` when a file shrank below what the baseline holds or went from it, or that there is nothing to do.
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
    /// - A pointer is there only if git tracks it: a file the build makes, or one `.gitignore` hides, is `broken_pointer`. Name such a file, or one that does not exist yet, without backticks; a name with a `/` under no tracked directory, such as `models/` the build makes, is not a pointer and keeps its backticks.
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
    /// Adopts docfit on a repo in one command: registers each Markdown file by the kind its place says, holds the rest to rehome, formats the documents, and writes the baseline at today's sizes and broken pointers, so `docfit check` is green that day.
    ///
    /// ## When to run it
    ///
    /// Once, when a repo adopts docfit; then register each file it holds to rehome under a kind where one fits it, and run `docfit check`. Run again, it changes nothing already registered: it only registers files added since whose place says their kind.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads; without a `{const:CONFIG}` it starts one. It adds a line `"<path>" = "<kind>"` to the `[documents]` table for each Markdown file git tracks that no glob matches and whose place says its kind: `README.md` is `readme`; `CLAUDE.md`, `AGENTS.md` and a file in `docs/agents/` are `agent`; a file in `adr/` (such as `docs/adr/`) is `adr`; `GLOSSARY.md` is `glossary`. Every line already in `{const:CONFIG}`, and its comments, are kept as they are. Without a baseline yet, it first formats every registered document as `docfit fmt` does, then writes `{const:BASELINE}` as `docfit ratchet` does on a repo without one: each document over its budget held at its size now, formatted, each pointer broken now held in its document, and each Markdown file registered under no kind held to rehome at its size now. With a baseline, it leaves it as it is: once a repo has adopted docfit, its baseline only tightens. Each file is written whole, through a file beside it renamed over it, and only when it changes.
    ///
    /// ## What it answers
    ///
    /// - `registered`: each file it registered this run, with its kind. Empty on a run that adds none.
    /// - `rehome`: each file it held to rehome this run: `file`, `size` (its bytes), `kinds` (every kind, since its place says none) and `fix`, the line that registers it under a kind instead. docfit does not guess its kind.
    /// - `formatted`: each document it formatted this run, as `docfit fmt` answers it. Empty on a repo that has a baseline: it formats only when it adopts.
    /// - `faults`: what `docfit check` would answer now, as it answers it; empty on a repo it adopted this run.
    /// - `baseline`: the baseline now.
    /// - `changed`: whether it wrote `{const:CONFIG}` or the baseline. `false` is nothing to adopt.
    /// - `next`: the first fault's fix; with none, commit `{const:CONFIG}` and the baseline, or that there is nothing to adopt.
    ///
    /// For example, on a repo with `README.md` and `SPEC.md`:
    ///
    /// ```json
    /// {
    ///   "registered": { "README.md": "readme" },
    ///   "rehome": [
    ///     { "file": "SPEC.md", "size": 15360, "kinds": ["readme", "agent", "adr", "glossary", "design-note"], "fix": "add `\"SPEC.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note" }
    ///   ],
    ///   "formatted": ["README.md"],
    ///   "faults": [],
    ///   "baseline": { "README.md": { "bytes": 65536, "pointers": ["docs/old.md"] }, "SPEC.md": { "rehome": 15360 } },
    ///   "changed": true,
    ///   "next": "commit docfit.toml and .docfit/baseline.json, then `docfit check`; register a file in `rehome` under a kind where one fits it"
    /// }
    /// ```
    ///
    /// `docfit check` is then green on the repo as it is, apart from a file registered under two kinds, which it leaves for you. It exits {const:EXIT_OK}.
    ///
    /// ## What it may refuse
    ///
    /// What `docfit check` refuses, apart from a missing `{const:CONFIG}`; a `[documents]` written other than as a table header, when it has a line to add; and a file it cannot write. It exits {const:EXIT_ERROR}.
    ///
    /// ## What trips you up
    ///
    /// - A file held to rehome is not a document, so not measured against a budget: registered later under a kind and over its budget, `docfit check` is red on it until it is cut.
    /// - On a repo that has a baseline, a Markdown file added since and registered under no kind is not held to rehome: `docfit check` is red on it until it is registered.
    /// - It rewrites documents that hold more than one sentence on a line: review that diff with the rest. It formats before it measures, so a document's budget is held at its formatted size, and `docfit fmt` after it does not push one over.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Init {
        /// The repository to adopt docfit on (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
    /// Moves the baseline down to the documents' sizes now; it never moves it up.
    ///
    /// ## When to run it
    ///
    /// Once, when a repo whose `docfit.toml` is written adopts docfit with documents over their budgets (`docfit init` does it for a repo without one): it holds each at its size now, so `docfit check` is green that day. After that, with a change that shrinks a document held over its budget, when `docfit check`'s `next` says so; commit `{const:BASELINE}` with the change.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads, and writes `{const:BASELINE}` (whole, through a file beside it renamed over it) when the baseline moves. With no baseline yet, it holds each document over its budget, in each measure it is over, at its size now, and each Markdown file registered under no kind to rehome, at its size in bytes now. With one, a file held to rehome is held at the smaller of what is held and its size now, and leaves it once it is gone or registered under a kind; a file registered under no kind it does not hold is never added; each measure held moves to the smaller of what is held and the size now; a measure it does not hold stays unheld, so a document that grows over its budget is never added; a measure back within budget leaves it. Broken pointers likewise: with no baseline yet, each pointer broken now is held in its document; with one, a held pointer leaves once it is fixed (or its document goes), and a pointer broken since is never added.
    ///
    /// ## What it answers
    ///
    /// - `changed`: whether the baseline moved. `false` is nothing to ratchet, and it writes nothing.
    /// - `baseline`: the baseline now, each held document with its held `bytes`, `line` and `pointers`.
    /// - `next`: commit the baseline, or that there is nothing to ratchet.
    ///
    /// It exits {const:EXIT_OK}.
    ///
    /// ## What it may refuse
    ///
    /// What `docfit check` refuses, and a baseline it cannot write; it exits {const:EXIT_ERROR} and writes nothing. A file not registered is not refused.
    ///
    /// ## What trips you up
    ///
    /// - Deleting the baseline and running it again holds every document at its size now, every broken pointer, and every file registered under no kind to rehome: that is adopting docfit again, and shows in the diff.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Ratchet {
        /// The repository to ratchet (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
    /// Breaks the prose of every registered document at its sentences, one sentence per line.
    ///
    /// One sentence per line (semantic line breaks) means two changes that edit different sentences of one paragraph edit different lines, so they never conflict when merged.
    ///
    /// ## When to run it
    ///
    /// When `docfit check` names an `unformatted` or a `line_too_long` fault, or after writing prose. It is safe to run any time: a second run changes nothing.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads, without the baseline. With no files named it formats every registered document; with files named, only those, each named from the repo's top as `docfit check` lists it. It rewrites each document it changes, whole, through a file beside it renamed over it, and leaves the others untouched.
    ///
    /// A sentence ends at `.`, `?` or `!` followed by a space, or at `。`, `？` or `！`; a closing `)`, `]`, quote, `*` or `_` right after the end stays with its sentence. Where more of the line follows a sentence's end, the line is broken there and the spaces between the two sentences dropped; the new line repeats the line's indent and quote markers, and a list item's later lines are indented to its text. It leaves as they are: front matter, fenced code blocks, tables, headings, HTML lines, link definitions, inline code, and a break before text that would start a block of its own (a list marker, `#`, `>`, `|`). It only breaks lines, never joins them.
    ///
    /// ## What it answers
    ///
    /// - `changed`: whether it changed any document. `false` is nothing to format, and it writes nothing.
    /// - `formatted`: the documents it changed, in path order; empty when every one was formatted already.
    /// - `next`: review the diff and commit it, or that there is nothing to format.
    ///
    /// It exits {const:EXIT_OK}.
    ///
    /// ## What it may refuse
    ///
    /// What `docfit check` refuses, except a baseline; a file named that is not a registered document (the fix: name it as `docfit check` lists it, or register it); and a document it cannot write. It exits {const:EXIT_ERROR}; a refusal before it writes writes nothing.
    ///
    /// ## What trips you up
    ///
    /// - An abbreviation such as `e.g.` followed by a space ends a sentence to it: write `for example`.
    /// - An indented code block is prose to it: fence code with ``` instead.
    /// - Killed while writing, each document is whole, either as it was or formatted: run it again.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Fmt {
        /// The documents to format, from the repo's top (default: every registered document).
        #[arg(verbatim_doc_comment)]
        files: Vec<String>,
        /// The repository to format (default: the one containing the current directory).
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
        .constant("CONFIG", CONFIG)
        .constant("ADRS", pointers::ADRS)
        .constant("EXTENSIONS", pointers::extensions())
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
        Cmd::Init { repo } => init(repo).unwrap_or_else(refused),
        Cmd::Ratchet { repo } => ratchet(repo).unwrap_or_else(refused),
        Cmd::Fmt { repo, files } => fmt(repo, files).unwrap_or_else(refused),
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

fn top(repo: Option<PathBuf>) -> Result<PathBuf, Vec<Refusal>> {
    repo::top(&repo.unwrap_or_else(|| PathBuf::from(".")))
}

/// The repo's `docfit.toml`: `None` when it has none.
fn config(top: &Path) -> Result<Option<String>, Vec<Refusal>> {
    match std::fs::read_to_string(top.join(CONFIG)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(vec![refusal(
            &format!("cannot read {CONFIG} at the repo's top: {e}"),
            &format!("make {CONFIG} readable UTF-8 text, or restore it from git"),
        )]),
    }
}

/// The repo's top, its documents registered, and the paths git tracks.
fn registered(repo: Option<PathBuf>) -> Result<(PathBuf, Answer, Vec<String>), Vec<Refusal>> {
    let top = top(repo)?;
    let config = config(&top)?.ok_or_else(|| {
        vec![refusal(
            &format!("there is no {CONFIG} at the repo's top"),
            &format!("run `docfit init`, or write {CONFIG} with a `[{TABLE}]` table, a line `\"<glob>\" = \"<kind>\"` per document (see `docfit check --help`)"),
        )]
    })?;
    let entries = documents::entries(&config)?;
    let tracked = repo::tracked(&top)?;
    let answer = documents::check(&entries, &tracked);
    Ok((top, answer, tracked))
}

/// The registered documents named in `files` (every one, when it names
/// none), each with its text read from the working tree.
fn read<'a>(
    top: &Path,
    answer: &'a Answer,
    files: &[String],
) -> Result<Vec<(&'a str, Document, String)>, Vec<Refusal>> {
    let mut refusals: Vec<Refusal> = files
        .iter()
        .filter(|f| !answer.documents.contains_key(*f))
        .map(|f| {
            refusal(
                &format!("`{f}` is not a registered document"),
                &format!(
                    "name it from the repo's top as `docfit check` lists it under `documents`, or register it in {CONFIG}"
                ),
            )
        })
        .collect();
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let mut texts = Vec::new();
    let named = answer
        .documents
        .iter()
        .filter(|(f, _)| files.is_empty() || files.contains(f));
    for (file, doc) in named {
        match std::fs::read_to_string(top.join(file)) {
            Ok(text) => texts.push((file.as_str(), *doc, text)),
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

/// Each document read, measured, with its pointers to what is not in `tree`.
fn measured<'a>(texts: &'a [(&'a str, Document, String)], tree: &Tree) -> Vec<Measured<'a>> {
    texts
        .iter()
        .map(|(file, doc, text)| {
            let (size, at) = Size::of(text);
            Measured {
                file,
                budget: doc.budget,
                size,
                at,
                broken: pointers::check(file, text, tree),
            }
        })
        .collect()
}

/// Each tracked Markdown file registered under no kind, measured, with the
/// `unregistered` fault `answer` holds on it.
fn strays<'a>(top: &Path, answer: &'a Answer) -> Result<Vec<Stray<'a>>, Vec<Refusal>> {
    let (mut strays, mut refusals) = (Vec::new(), Vec::new());
    for fault in answer
        .faults
        .iter()
        .filter(|f| f["fault"] == "unregistered")
    {
        let file = fault["file"].as_str().expect("a fault names its file");
        match std::fs::metadata(top.join(file)) {
            Ok(meta) => strays.push(Stray {
                file,
                bytes: meta.len() as usize,
                fault,
            }),
            Err(e) => refusals.push(refusal(
                &format!("cannot read `{file}`, which git tracks: {e}"),
                &format!("restore `{file}`, or `git rm` it"),
            )),
        }
    }
    if refusals.is_empty() {
        Ok(strays)
    } else {
        Err(refusals)
    }
}

/// Every fault, in path order: `answer`'s that are not a stray's, those
/// `budget::check` finds in `docs` and `strays` against `baseline`, and each
/// document's unformatted prose in `texts`.
fn faults(
    answer: &Answer,
    texts: &[(&str, Document, String)],
    docs: &[Measured],
    strays: &[Stray],
    baseline: Option<&Baseline>,
) -> Vec<Value> {
    let mut faults: Vec<Value> = answer
        .faults
        .iter()
        .filter(|f| f["fault"] != "unregistered")
        .cloned()
        .collect();
    faults.extend(budget::check(docs, strays, baseline));
    for (file, _, text) in texts {
        if let (_, Some(line)) = sentences::format(text) {
            faults.push(json!({
                "file": file, "fault": "unformatted", "line": line,
                "fix": format!("run `docfit fmt {file}`: it breaks its prose at its sentences, one per line"),
            }));
        }
    }
    faults.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
    faults
}

/// The baseline at the repo's top: `None` when it has none yet.
fn baseline(top: &Path) -> Result<Option<Baseline>, Vec<Refusal>> {
    match std::fs::read_to_string(top.join(BASELINE)) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|e| {
            vec![refusal(
                &format!("{BASELINE} is not a baseline: {e}"),
                &format!("restore {BASELINE} from git, or delete it and run `docfit ratchet`"),
            )]
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(vec![refusal(
            &format!("cannot read {BASELINE}: {e}"),
            &format!("make {BASELINE} readable, or restore it from git"),
        )]),
    }
}

fn check(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let (top, answer, tracked) = registered(repo)?;
    let texts = read(&top, &answer, &[])?;
    let docs = measured(&texts, &Tree::new(&tracked));
    let strays = strays(&top, &answer)?;
    let held = baseline(&top)?;
    let faults = faults(&answer, &texts, &docs, &strays, held.as_ref());
    let tighter = held
        .as_ref()
        .is_some_and(|h| &budget::ratchet(&docs, &strays, Some(h)) != h);
    let (code, next) = match faults.first() {
        Some(first) => (EXIT_FAULTS, first["fix"].clone()),
        None if tighter => (
            EXIT_OK,
            json!(format!(
                "run `docfit ratchet`: a file shrank or went, and {BASELINE} can hold it tighter"
            )),
        ),
        None => (
            EXIT_OK,
            json!(
                "nothing to do: every Markdown file is registered under one kind, within its budget, one sentence per line, or held to rehome"
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

fn init(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let top = top(repo)?;
    let config = config(&top)?;
    let files = repo::tracked(&top)?;
    let (text, added) = documents::adopt(config.as_deref(), &files)?;
    let answer = documents::check(&documents::entries(&text)?, &files);
    let mut texts = read(&top, &answer, &[])?;
    let strays = strays(&top, &answer)?;
    let tree = Tree::new(&files);
    let held = baseline(&top)?;
    let mut formatted = Vec::new();
    if held.is_none() {
        for (file, _, text) in texts.iter_mut() {
            if let (new, Some(_)) = sentences::format(text) {
                write(&top.join(*file), &new)?;
                *text = new;
                formatted.push(*file);
            }
        }
    }
    let docs = measured(&texts, &tree);
    let (next, adopted) = match held {
        Some(held) => (held, false),
        None => {
            let next = budget::ratchet(&docs, &strays, None);
            write(&top.join(BASELINE), &(pretty(&json!(next)) + "\n"))?;
            (next, true)
        }
    };
    let wrote_config = config.as_deref() != Some(text.as_str());
    if wrote_config {
        write(&top.join(CONFIG), &text)?;
    }
    let rehome: Vec<Value> = strays
        .iter()
        .filter(|_| adopted)
        .map(|s| json!({ "file": s.file, "size": s.bytes, "kinds": s.fault["kinds"], "fix": s.fault["fix"] }))
        .collect();
    let faults = faults(&answer, &texts, &docs, &strays, Some(&next));
    let changed = adopted || wrote_config;
    let then = match faults.first() {
        Some(first) => first["fix"].clone(),
        None if changed => json!(format!(
            "commit {CONFIG} and {BASELINE}, then `docfit check`; register a file in `rehome` under a kind where one fits it"
        )),
        None => json!("nothing to adopt: run `docfit check`"),
    };
    let registered: std::collections::BTreeMap<_, _> =
        added.iter().map(|(f, k)| (f, k.name())).collect();
    Ok((
        pretty(&json!({
            "registered": registered, "rehome": rehome, "formatted": formatted, "faults": faults, "baseline": next,
            "changed": changed, "next": then,
        })),
        EXIT_OK,
    ))
}

fn ratchet(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let (top, answer, tracked) = registered(repo)?;
    let texts = read(&top, &answer, &[])?;
    let docs = measured(&texts, &Tree::new(&tracked));
    let held = baseline(&top)?;
    let strays = strays(&top, &answer)?;
    let next = budget::ratchet(&docs, &strays, held.as_ref());
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

fn fmt(repo: Option<PathBuf>, files: Vec<String>) -> Result<(String, i32), Vec<Refusal>> {
    let (top, answer, _) = registered(repo)?;
    let mut formatted = Vec::new();
    for (file, _, text) in read(&top, &answer, &files)? {
        if let (new, Some(_)) = sentences::format(&text) {
            write(&top.join(file), &new)?;
            formatted.push(file);
        }
    }
    let next = if formatted.is_empty() {
        "nothing to format: run `docfit check`"
    } else {
        "review the diff and commit it, then `docfit check`"
    };
    Ok((
        pretty(&json!({ "changed": !formatted.is_empty(), "formatted": formatted, "next": next })),
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
    let dir = path.parent().expect("a file is in a directory");
    std::fs::create_dir_all(dir).map_err(failed)?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
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
