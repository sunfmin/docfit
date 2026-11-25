//! The command line, read against the manual: its commands, the manual's
//! pages, and what docfit exits with.

use crate::budget::{self, BASELINE, Baseline, Measured, Size, Stray};
use crate::documents::{self, Answer, CATALOGS, CONFIG, Config, Document, Refusal, TABLE};
use crate::kind::Kind;
use crate::paragraph::{self, Adr, Home, Known};
use crate::pointers::{self, Tree};
use crate::refs::{self, Found};
use crate::repo;
use crate::restates::{self, Index, Restates, Text};
use crate::tasks;
use agent_help::{Help, Topic};
use clap::{FromArgMatches, Parser, Subcommand};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
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
/// A repo declares its documents in `docfit.toml`, at its top: each Markdown file git tracks, unless it ignores it (see `docfit check --help`), is registered by a glob under one kind, and a kind says who reads the file and what it holds, nothing else. The kinds are fixed in docfit:
///
/// {const:KINDS}
///
/// Promises live in ADRs: there is no spec kind.
///
/// ## Budgets
///
/// Each document has a budget: its size in bytes, and its longest paragraph in characters. A paragraph is a run of non-blank lines outside fenced code, its lines joined on a space; a list item and a table row are each one, and a heading is none. A budget on paragraphs catches the one that grows without end, and leaves prose as it is written: split a paragraph over its budget, or move part of it to its home. The table above gives each kind's default; a `docfit.toml` entry may set its own (see `docfit check --help`). `docfit check` is red on a document over its budget, unless the baseline, `{const:BASELINE}`, holds it at more: then it is red only once it grows past what the baseline holds. `docfit ratchet` moves the baseline down to the documents' sizes now, and never up. A Markdown file no kind fits (there is no spec kind) may be held **to rehome**: `docfit check` is green on it while it does not grow, and red once it does; `docfit ratchet` only shrinks what it is held at, and drops it once the file is gone or registered under a kind. A pointer broken when a repo adopts docfit is held too, by its document and the pointer: `docfit check` is green on it, and red on a pointer broken since; `docfit ratchet` drops it once it is fixed, and never adds one. So a repo adopting docfit with a document far over its budget, a broken pointer, or a file no kind fits, runs `docfit init` (or, with its `docfit.toml` written, `docfit ratchet`) once and is green that day, and every change after can only shrink it.
///
/// ## Pointers
///
/// What a document points at stays true: each repo path it names in backticks or links to, and each ADR it names by number (`ADR 0007` is a file `{const:ADRS}0007-*`), must be a path git tracks, or a directory holding one. `docfit check` is red on one that is not, naming the document, the line and what is missing. A pointer that breaks shows at once; a copy that goes stale shows nowhere, so point rather than copy: `docfit check` is red on a new copy of a catalog's string or of another file's lines (`docfit check --help`, Restatements).
///
/// ## Output
///
/// Every command prints one JSON object on stdout with a `next` field: what to do next. A command that finds faults exits {const:EXIT_FAULTS}; each fault carries its `fix`. Nothing wrong exits {const:EXIT_OK}. A command that cannot finish exits {const:EXIT_ERROR} with `{"error": [{"fault": "...", "fix": "..."}, ...]}`: every fault it found, each with the command or edit that fixes it. Do every fix, then run the command again.
#[derive(Parser)]
#[command(
    name = "docfit",
    version = env!("DOCFIT_VERSION"),
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
    /// Checks that every Markdown file git tracks is registered in `docfit.toml` under one kind, within its budget, and that what it points at is there; that no document restates a catalog's string or another file's lines anew; and that no tracked text file cites the local tracker anew.
    ///
    /// ## When to run it
    ///
    /// In the repo's gate, and after adding, moving, renaming or growing a Markdown file. It reads git's index, `docfit.toml`, the baseline and the repo's text files, so it takes well under a second.
    ///
    /// ## What it reads and writes
    ///
    /// It reads `docfit.toml` at the top of the repo, and the paths git tracks there (`git ls-files`): every one ending `.md` is a document. A file on disk that git does not track is not. It reads each registered document's text from the working tree, every other text file git tracks that `ignore` does not match (for restatements and local tracker refs, below), the string catalogs `catalogs` lists (`docfit where --help`), and the baseline `{const:BASELINE}` if there is one. It writes nothing. `docfit.toml` holds one table, `[documents]`, a line per glob, the glob a path from the repo's top and the kind one of the kinds in `docfit --help`, or a table that also sets the document's budget, `bytes` (its size) or `paragraph` (its longest paragraph, in characters) or both:
    ///
    /// ```toml
    /// [documents]
    /// "README.md" = { kind = "readme", bytes = 6144 }
    /// "CLAUDE.md" = "agent"
    /// "docs/agents/*.md" = "agent"
    /// "docs/adr/*.md" = { kind = "adr", paragraph = 400 }
    /// ```
    ///
    /// In a glob, `*` and `?` stay within one directory and `**` crosses directories, as in a `.gitignore`. A measure no matching entry sets is the kind's default; where several set it, the smallest holds. A kind with no default size (`product`) bounds a document's size only where an entry sets `bytes`.
    ///
    /// Markdown that is not a document, such as help a program embeds from `src/` or a test's fixtures, is code or data: `docfit.toml` ignores it by glob, in a list before its first table. A path an `ignore` glob matches is neither a document nor a file to rehome, whatever `[documents]` says: no check reads it, and the baseline holds nothing for it. Every repo ignores {const:IGNORED}, the fits' own records; its list adds to those:
    ///
    /// ```toml
    /// ignore = ["src/**", "tests/fixtures/**"]
    /// ```
    ///
    /// The baseline maps a document to the measures it is held at over its budget and the pointers it is held at broken, a document to the catalog strings and copied runs it is held at (`strings`, `copies`), a file registered under no kind to the bytes it is held at to rehome, and any file to the local refs it is held at, as `docfit ratchet` writes it: `{"README.md": {"bytes": 65536, "paragraph": 18000, "pointers": ["docs/old.md"], "strings": 84}, "SPEC.md": {"rehome": 15360}, "src/a.rs": {"local_refs": 4}}`. A document within its budget is held at its budget. A pointer is held by what it is written as, its `pointer`: the same pointer written again in that document is held too.
    ///
    /// ## Restatements
    ///
    /// A copy of what the code or a string catalog already says gives one fact two homes, and goes stale without anyone noticing. `docfit check` is red, with the fault `restates`, on a document holding more of either of these than the baseline holds it at (none, unless held):
    ///
    /// - `strings`: a string quoted outside fenced code and code spans that a catalog holds verbatim, as `docfit where` finds it (`docfit where --help`). Without `catalogs`, there is none. Point at the key, or at the screen test that shows it, instead.
    /// - `copies`: a run of {const:RUN} or more non-blank lines, each trimmed and blank lines between them skipped, that another text file git tracks and `ignore` does not match holds in the same order: code, or another document. A run in a fenced block that names its source, by its path or its file name, on its opening line or on the non-blank line before the block, is no copy of that file. A file that is a symbolic link is not read, so it restates nothing and nothing restates it. Keep the run in one place and point at it.
    ///
    /// A code span naming what is in the code is a pointer, the way a document should point at code: `docfit where` lists it, and it is never red. `docfit where` and `docfit check` find restatements by the one detector. The baseline holds each document's count of each, shrink-only: `docfit init` holds each document's counts when it adopts docfit, and of a measure a baseline holds no document's count of yet (so adding `catalogs` later, then running `docfit init`, holds the strings); `docfit ratchet` lowers a count to the count now, drops it at 0, and never adds one.
    ///
    /// ## Local tracker refs
    ///
    /// Content a repo keeps never cites the local tracker: its issues live in one host's database, so a ref to one is a pointer no later reader can follow. `docfit.toml` names the tracker's refs, before its first table: `{const:LOCAL_REFS} = ["rollmind", "factorium"]`, each a name whose `<name>#<digits>` (`rollmind#38`) cites the local tracker; whoever wires docfit into the repo writes it. Without it this check is off, and `next` says how to turn it on. It reads every text file git tracks that `ignore` does not match ({const:IGNORED} among them: the fits' records are their bookkeeping), Markdown or not; a file with a NUL byte in its first 8000 is binary, and not read, and so is one it cannot read or a symbolic link. A ref is `<name>#<digits>` with a listed name, not run on by a letter, digit, `-`, `_` or `.` before it or a letter, digit or `_` after it; one with an owner (`sunfmin/lore#25`) or in a URL (a word with `://`) is not local. A commit message is not read: it may carry an issue's id, as long as it reads alone. `docfit check` is red on a file with more refs than the baseline holds it at (none, unless held). The baseline holds a file's count shrink-only: `docfit init` holds each file's count when it adopts docfit, or on a baseline that holds no refs yet; `docfit ratchet` (holding each as `docfit init` does on a repo without a baseline) lowers it to the count now, drops it at 0, and never adds a file it does not hold.
    ///
    /// ## What it answers
    ///
    /// - `documents`: each registered document's path, with its kind.
    /// - `faults`: in path order, each file that is not registered under exactly one kind, and each document over what it may be. Empty when there is none. Each has `file`, `fault` and `fix`:
    ///   - `unregistered`: no glob matches the file. `kinds` are the kinds it could be: the one its place says, such as `agent` for `CLAUDE.md` or a file in `docs/agents/`, otherwise every kind; `fix` is the line to add.
    ///   - `two_kinds`: globs of several kinds match it; `kinds` are those kinds.
    ///   - `rehome_grew`: a file registered under no kind, held to rehome, is `size` bytes, over `limit`, the bytes the baseline holds it at. Held to rehome, a file registered under no kind is no `unregistered` fault.
    ///   - `over_budget`: its size in bytes, `size`, is over `limit`, its budget or, when the baseline holds it at more, that.
    ///   - `paragraph_too_long`: its paragraph starting at line `line` is `size` characters, over `limit`, as above. The fix is to split it, or to move part of it to its home (`docfit where` suggests one).
    ///   - `broken_pointer`: its line `line` points, as `pointer`, at `missing`, which git does not track, and the baseline does not hold that pointer broken in that document. A pointer is:
    ///     - a backticked repo path: with a `/` in it, or a name ending in one of {const:EXTENSIONS}, and none of a command's space, a glob's `*`, a placeholder's `<>`, a URL's `:` (but a `:<line>` after it), or a leading `/` or `~`. A name with a `/` and none of those extensions is a pointer only when its first segment (`src` of `src/a/b`) is something git tracks at the repo's top or in the document's directory; otherwise it is a name, such as a model id `org/model` or a git ref `origin/main`, and is not checked. A pointer is read from the repo's top or the document's directory, and is there if either is.
    ///     - a Markdown link's target, `[text](target)`, read from the document's directory (a leading `/` is the repo's top), without its `#anchor`. A URL (`https:`, `mailto:`) and an anchor alone are not checked.
    ///     - an ADR named by number, `ADR 0007` or `ADR-0007`: there must be a file `{const:ADRS}0007-*`.
    ///
    ///     A fenced code block points at nothing.
    ///   - `restates`: the document holds `size` restatements of the kind `restates` (`strings` or `copies`, see Restatements above), over `limit`, the count the baseline holds it at; `line` is where the first past that count starts (a string's paragraph), `found` what it is (the string, or the run's first line) and `from` where the repo already says it (the catalog, or `<file>:<line>` of the run copied).
    ///   - `local_ref`: any text file it reads (see Local tracker refs above) holds `size` refs, over `limit`, the count the baseline holds it at; `line` and `ref` are the first ref past that count, in file order. The fix: replace it with what it points at, or delete it.
    /// - `next`: the first fault's fix; with none, `docfit ratchet` when a file shrank below what the baseline holds or went from it, or that there is nothing to do.
    ///
    /// For example, with `CLAUDE.md` registered and `notes.md` not:
    ///
    /// ```json
    /// {
    ///   "documents": { "CLAUDE.md": "agent" },
    ///   "faults": [
    ///     { "file": "notes.md", "fault": "unregistered", "kinds": ["readme", "agent", "adr", "glossary", "design-note", "skill", "product"], "fix": "add `\"notes.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note, skill, product" }
    ///   ],
    ///   "next": "add `\"notes.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note, skill, product"
    /// }
    /// ```
    ///
    /// It exits {const:EXIT_FAULTS} when there is a fault, {const:EXIT_OK} when there is none.
    ///
    /// ## What it may refuse
    ///
    /// It exits {const:EXIT_ERROR}, naming every fault at once, outside a git repository, without a `docfit.toml` or its `[documents]` table, on a `docfit.toml` that is not TOML, on a glob that is not one, on an `ignore` that is not a list of globs, on a `{const:LOCAL_REFS}` that is not a list of names (letters, digits, `-`, `_`, `.`), on a `catalogs` that is not a list of globs or a glob in it that matches no tracked file, on a catalog it cannot read, on an entry that is neither a kind nor a table of `kind`, `bytes` and `paragraph`, on a kind docfit does not know (the fix lists the kinds), on a document it cannot read as UTF-8 text, and on a baseline that is not one.
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
    /// Adopts docfit on a repo in one command: ignores the Markdown under source and test directories, registers each Markdown file by the kind its place says, holds the rest to rehome, and writes the baseline at today's sizes and broken pointers, so `docfit check` is green that day.
    ///
    /// ## When to run it
    ///
    /// Once, when a repo adopts docfit; then register each file it holds to rehome under a kind where one fits it, and run `docfit check`. Run again, it changes nothing already registered or ignored: it only ignores and registers files added since, as below.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads; without a `{const:CONFIG}` it starts one. First it ignores Markdown that is code or a test's data: for each Markdown file git tracks that no glob ignores or registers, with one of {const:SOURCE} as a directory in its path, it adds the first such directory, as `<path>/**` (`crates/cli/src/**`), to `ignore`, unless an entry registers a file under it; the list is written again on one line where it was, or, without one, before the first table. Then it adds a line `"<path>" = "<kind>"` to the `[documents]` table for each Markdown file git tracks that no glob matches and whose place says its kind: `README.md` is `readme`; `CLAUDE.md`, `AGENTS.md` and a file in `docs/agents/` are `agent`; a file in `adr/` (such as `docs/adr/`) is `adr`; `GLOSSARY.md` is `glossary`; `SKILL.md`, and a file in a skill's directory `skills/<name>/` (such as its `references/`), is `skill`. No place says `product`: docfit does not guess it. Every other line already in `{const:CONFIG}`, and its comments, are kept as they are. Without a baseline yet, it writes `{const:BASELINE}` as `docfit ratchet` does on a repo without one: each document over its budget held at its size now, each pointer broken now held in its document, and each Markdown file registered under no kind held to rehome at its size now. With a baseline, it leaves it as it is: once a repo has adopted docfit, its baseline only tightens; but on one that holds no local tracker refs yet, it holds each file's refs at their count now (`docfit check --help`, Local tracker refs), and likewise each document's catalog strings, or its copied runs, on one that holds no document's count of them yet (Restatements there). Each file is written whole, through a file beside it renamed over it, and only when it changes.
    ///
    /// ## What it answers
    ///
    /// - `registered`: each file it registered this run, with its kind. Empty on a run that adds none.
    /// - `ignored`: each glob it added to `ignore` this run, `glob`, with the `files` it ignores. Look at each: a glob over documents is taken out of `ignore` by hand, and its files registered under a kind. Empty on a run that adds none.
    /// - `rehome`: each file it held to rehome this run: `file`, `size` (its bytes), `kinds` (every kind, since its place says none) and `fix`, the line that registers it under a kind instead. docfit does not guess its kind.
    /// - `faults`: what `docfit check` would answer now, as it answers it; empty on a repo it adopted this run.
    /// - `baseline`: the baseline now.
    /// - `changed`: whether it wrote `{const:CONFIG}` or the baseline. `false` is nothing to adopt.
    /// - `next`: the first fault's fix; with none, commit `{const:CONFIG}` and the baseline, or that there is nothing to adopt.
    ///
    /// For example, on a repo with `README.md`, `SPEC.md` and `src/help.md`:
    ///
    /// ```json
    /// {
    ///   "registered": { "README.md": "readme" },
    ///   "ignored": [{ "glob": "src/**", "files": ["src/help.md"] }],
    ///   "rehome": [
    ///     { "file": "SPEC.md", "size": 15360, "kinds": ["readme", "agent", "adr", "glossary", "design-note", "skill", "product"], "fix": "add `\"SPEC.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note, skill, product" }
    ///   ],
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
    /// What `docfit check` refuses, apart from a missing `{const:CONFIG}`; a `[documents]` written other than as a table header, when it has a line to add; an `ignore` written other than as a line `ignore = [...]` before the first table, when it has a glob to add; and a file it cannot write. It exits {const:EXIT_ERROR}.
    ///
    /// ## What trips you up
    ///
    /// - A file held to rehome is not a document, so not measured against a budget: registered later under a kind and over its budget, `docfit check` is red on it until it is cut.
    /// - On a repo that has a baseline, a Markdown file added since and registered under no kind is not held to rehome: `docfit check` is red on it until it is registered.
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
    /// It reads what `docfit check` reads, and writes `{const:BASELINE}` (whole, through a file beside it renamed over it) when the baseline moves. With no baseline yet, it holds each document over its budget, in each measure it is over, at its size now, and each Markdown file registered under no kind to rehome, at its size in bytes now. With one, a file held to rehome is held at the smaller of what is held and its size now, and leaves it once it is gone or registered under a kind; a file registered under no kind it does not hold is never added; each measure held moves to the smaller of what is held and the size now; a measure it does not hold stays unheld, so a document that grows over its budget is never added; a measure back within budget leaves it. Broken pointers likewise: with no baseline yet, each pointer broken now is held in its document; with one, a held pointer leaves once it is fixed (or its document goes), and a pointer broken since is never added. Local tracker refs likewise, by each file's count (`docfit check --help`); with the check off, a held count stays as it is. Restatements likewise, by each document's count of catalog strings and of copied runs.
    ///
    /// ## What it answers
    ///
    /// - `changed`: whether the baseline moved. `false` is nothing to ratchet, and it writes nothing.
    /// - `baseline`: the baseline now, each held document with its held `bytes`, `paragraph` and `pointers`.
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
    /// - A baseline written before docfit measured paragraphs holds a document's `line`, its longest line: that is read as holding no paragraph, so a paragraph over its budget is red, and the next run writes the baseline anew without it. To hold such paragraphs instead, delete the baseline and run it again (below).
    /// - Deleting the baseline and running it again holds every document at its size now, every broken pointer, and every file registered under no kind to rehome: that is adopting docfit again, and shows in the diff.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Ratchet {
        /// The repository to ratchet (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
    /// Lists a document by paragraph: what can be seen in each mechanically, and the home that suggests.
    ///
    /// ## When to run it
    ///
    /// Before moving a document's content to its homes, such as a README or a spec far over its budget: the facts are then in front of you, and only the judgement is left. It reads git and a few files, so it takes about a second.
    ///
    /// ## What it reads and writes
    ///
    /// It reads `<file>`, given from the current directory, and the repo it is in: `docfit.toml`, the paths git tracks, the text of every tracked file outside Markdown, the string catalogs, and the documents registered as `adr`. It writes nothing. `docfit.toml` may list the repo's string catalogs by glob, beside `[documents]`:
    ///
    /// ```toml
    /// catalogs = ["App/**/*.xcstrings"]
    /// ```
    ///
    /// A paragraph is a run of lines between blank lines; a list item and a table row are each one, and a fenced block is one in which nothing is read. A heading, a table's header and its separator are none. In each paragraph it sees:
    ///
    /// - `code`: the code spans, one word each, that are a path git tracks (a file or a directory) or that a tracked text file not ending `.md` holds as a whole word, as `git grep -w` finds it (a file with a NUL byte in its first 8000 is binary, and not read).
    /// - `strings`: the strings quoted with `"…"`, `“…”`, `「…」` or `『…』` that a catalog holds verbatim as a quoted string.
    /// - `copies`: where each run it starts that is copied from another tracked file is, `<file>:<line>`, as `docfit check` finds it (`docfit check --help`, Restatements). Listed only: it suggests no home, since either copy may be the one to keep.
    /// - `evidence`: numbers with units (`1.5 s`, `63 MiB`, `70 分钟`).
    /// - `local_refs`: references to the local tracker's issues (`#123`, `<checkout>#123`). Only this host can open them, so a repo's content never cites one.
    /// - `decision`: a choice or a reason (`because`, `chose`, `choose`, `因为`, `选`) beside an alternative (`instead of`, `rather than`, `而不是`, `而非`); both, or none is seen.
    /// - `pointer`: each of its parts, cut at `.`, `?`, `!`, `。`, `！`, `？` or `；`, names where something is and little else: a link or a tracked path in a code span, a word saying where (`see`, `in`, `at`, `under`, `lives`, `见`, `在`, `位于`, `→`) unless the part is only those, and at most 8 other words (a CJK character counts a half).
    ///
    /// Its home is the first of these that applies:
    ///
    /// {const:HOMES}
    ///
    /// ## What it answers
    ///
    /// - `file`: the document, from the repo's top; `kind`: its kind, or null when `docfit.toml` registers it under none.
    /// - `paragraphs`: in order, each with `line` (where it starts), `text` (its first 80 characters), `code`, `strings` and `copies` (each `{"count", "found"}`, distinct, in order), `evidence`, `local_refs` and `decision` (lists), `pointer`, `home`, and, with home `adr`, `adr`: the ADR it names by number (`ADR 0002`, `adr/0002-…`) or title, or null for a new one.
    /// - `homes`: how many paragraphs each home has.
    /// - `next`: move each paragraph to its home, then `docfit check`.
    ///
    /// For example:
    ///
    /// ```json
    /// {
    ///   "file": "README.md",
    ///   "kind": "readme",
    ///   "paragraphs": [
    ///     { "line": 3, "text": "Startup took 1.5 s (#12).", "code": { "count": 0, "found": [] }, "strings": { "count": 0, "found": [] }, "copies": { "count": 0, "found": [] }, "evidence": ["1.5 s"], "local_refs": ["#12"], "decision": [], "pointer": false, "home": "evidence" }
    ///   ],
    ///   "homes": { "keep": 0, "adr": 0, "delete": 0, "evidence": 1, "replace": 0, "judge": 0 },
    ///   "next": "move each paragraph to its home (keep 0, adr 0, delete 0, evidence 1, replace 0, judge 0; `docfit where --help` says what each is), then run `docfit check`"
    /// }
    /// ```
    ///
    /// It exits {const:EXIT_OK}.
    ///
    /// ## What it may refuse
    ///
    /// It exits {const:EXIT_ERROR}, naming every fault at once, on a file it cannot read as UTF-8 text or that is in no git repository, on what `docfit check` refuses in `docfit.toml`, on a `catalogs` that is not a list of globs, on a glob in it that is not one or that matches no tracked file, and on a catalog or an ADR it cannot read.
    ///
    /// ## What trips you up
    ///
    /// - What is seen is mechanical: a home is a suggestion, and the features are there for you to judge it by. A paragraph holding several things shows them all, under the first home.
    /// - A code span is looked for in every tracked file outside Markdown, so a common word (`next`) is found anywhere.
    /// - Without `catalogs`, no string is found.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Where {
        /// The document, from the current directory.
        #[arg(verbatim_doc_comment)]
        file: PathBuf,
    },
    /// Lists the repo's document debt as tasks: each file the baseline holds to rehome, and each document it holds over its budget, with the steps that pay it.
    ///
    /// ## When to run it
    ///
    /// To find the next document to move to its homes. A factorium `[idle]` tasks line runs `docfit next --all` and files the first task of each kind as an issue, so the debt a repo adopted docfit with becomes work, one file at a time: `tasks = "slimfit --version && truefit --version && docfit --version && slimfit next --all && truefit next --all && docfit next --all"`. It reads git and a few files, so it takes about a second.
    ///
    /// ## What it reads and writes
    ///
    /// It reads what `docfit check` reads, and, for each task's file, what `docfit where` reads. It writes nothing.
    ///
    /// ## What it answers
    ///
    /// The shape of `slimfit next --all`'s and `truefit next --all`'s:
    ///
    /// - `by`: the docfit that read them, `docfit <version>`.
    /// - `counts`: how many tasks of each kind there are.
    /// - `tasks`, with `--all`: every task, rehome first, then over-budget, each in path order; without it, `task`: the first, or null. Each has:
    ///   - `kind`: `rehome`, a file the baseline holds to rehome that is still registered under no kind; or `over-budget`, a document the baseline holds over its budget that is still over it.
    ///   - `summary`: the file, and what it is.
    ///   - `files`: the one file, from the repo's top.
    ///   - `steps`: how to move its content to its homes; the text `docfit help rehome` prints.
    ///   - `accept`: what holds it done: after `docfit ratchet`, `docfit check` exits {const:EXIT_OK} and `{const:BASELINE}` no longer holds the file (gone, registered under a kind, or within its budget).
    ///   - `evidence`: `size` (a file's bytes; a document's `bytes` and `paragraph`), `held` (what the baseline holds it at), for a document its `budget`, and `where`: `docfit where`'s answer on the file.
    /// - `notes`: what was not looked for; without a baseline nothing is held, so there is no task.
    /// - `next`: do the first task by its steps, or that there is nothing to do.
    ///
    /// For example, with `SPEC.md` held to rehome (`where` cut short):
    ///
    /// ```json
    /// {
    ///   "by": "docfit 0.1.0",
    ///   "counts": { "rehome": 1, "over-budget": 0 },
    ///   "tasks": [
    ///     { "kind": "rehome", "summary": "`SPEC.md` (15360 bytes) is registered under no kind, held to rehome: move its content to its homes", "files": ["SPEC.md"], "steps": "…", "accept": "…", "evidence": { "size": 15360, "held": 15360, "where": { "file": "SPEC.md", "kind": null, "paragraphs": ["…"] } } }
    ///   ],
    ///   "notes": [],
    ///   "next": "do the first task by its steps, then its accept, and commit; then `docfit next` again"
    /// }
    /// ```
    ///
    /// It exits {const:EXIT_OK} with tasks or without.
    ///
    /// ## What it may refuse
    ///
    /// What `docfit check` refuses, and what `docfit where` refuses on a task's file; it exits {const:EXIT_ERROR}.
    ///
    /// ## What trips you up
    ///
    /// - A file the baseline holds that is gone, registered since, or back within its budget is no task: `docfit check`'s `next` says to run `docfit ratchet`, which drops it.
    /// - A file is a task until `docfit ratchet` drops it from the baseline: cutting it is not enough, commit the baseline with the change.
    #[command(
        next_help_heading = "Run by the agent, and by factorium's idle work",
        verbatim_doc_comment
    )]
    Next {
        /// Every task, as `tasks`, rather than the first.
        #[arg(long, verbatim_doc_comment)]
        all: bool,
        /// The repository to read (default: the one containing the current directory).
        #[arg(long, verbatim_doc_comment)]
        repo: Option<PathBuf>,
    },
    /// Shows one topic of this manual, or all of it.
    ///
    /// `docfit help rehome` shows how to move a document's content to its homes, the steps of every `docfit next` task; `docfit help all` shows the whole manual.
    #[command(next_help_heading = "Run by the agent", verbatim_doc_comment)]
    Help {
        /// The topic, or `all`.
        #[arg(verbatim_doc_comment)]
        topic: Option<String>,
    },
}

/// How to move a document's content to its homes, and so pay a `docfit next` task: a file held to rehome, or a document over its budget. These are the steps of every task `docfit next` answers.
///
/// 1. Run `docfit where <file>` and go through its paragraphs one by one. Each `home` is a suggestion; the features beside it are there to judge it by.
/// 2. Put each paragraph in its home:
///    - Code or strings (`delete`): delete it; the code, the catalog and the screen tests already say it.
///    - A decision (`adr`): an ADR, the one it names if that exists, else a new one, written as the repo's domain docs say.
///    - A number (`evidence`): the ADR of the decision it supports, with the command that measures it. A number that only proves the work was accepted goes in a comment on the issue you are working, not in the repo.
///    - A local tracker reference, `#N` or `<checkout>#N` (`replace`): replace it with what it points at (the person's words with who, date and where; or the command and what it printed), or delete it.
///    - A pointer (`keep`): keep it, in a document of its kind.
///    - A promise: an ADR; where the repo has truefit, register it there too.
///    - Anything else (`judge`): move it to the document whose kind holds it (`docfit --help`), or delete it.
/// 3. The commit that removes the content carries the map in its message: for each paragraph, its first words, then its home, which is one of: `file:line` where it now is; the ADR; the code symbol, catalog key or screen test that already says it; or "nowhere, because <it restated X>". So whoever finds the removed text in `git log` reads the map in the same commit. The map lives only in that commit message, never in a file of the repo: it is the signpost of one move, and the content's new homes are found without it. Like everything the repo keeps, it cites no issue of the local tracker.
/// 4. Run `docfit ratchet`, then `docfit check`, and commit the baseline with the change: the task is done once the baseline no longer holds the file.
#[derive(Topic)]
#[topic(name = "rehome")]
struct RehomeTopic;

pub(crate) fn help() -> Help {
    Help::new(<Cli as clap::CommandFactory>::command())
        .topic::<RehomeTopic>()
        .constant("KINDS", Kind::table())
        .constant("HOMES", Home::table())
        .constant("BASELINE", BASELINE)
        .constant("CONFIG", CONFIG)
        .constant("IGNORED", quoted(&documents::IGNORED))
        .constant("LOCAL_REFS", refs::KEY)
        .constant("RUN", restates::RUN)
        .constant("SOURCE", quoted(&documents::SOURCE))
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
        Cmd::Where { file } => place(&file).unwrap_or_else(refused),
        Cmd::Next { all, repo } => next(repo, all).unwrap_or_else(refused),
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

/// The text of each of `files` at `top`: one that is a symbolic link, that
/// cannot be read, or that holds a NUL byte in its first 8000 (binary), is
/// not read.
fn texts<'a>(top: &Path, files: impl Iterator<Item = &'a String>) -> Vec<Text> {
    files
        .filter(|f| std::fs::symlink_metadata(top.join(f)).is_ok_and(|m| !m.is_symlink()))
        .filter_map(|f| Some((f, std::fs::read(top.join(f)).ok()?)))
        .filter(|(_, bytes)| !bytes.iter().take(8000).any(|b| *b == 0))
        .map(|(f, bytes)| Text {
            path: f.clone(),
            text: String::from_utf8_lossy(&bytes).into_owned(),
        })
        .collect()
}

/// The local refs in each of `kept`, the text files `config` does not
/// ignore; `None` when it lists no `local_issue_refs`.
fn found(config: &Config, kept: &[Text]) -> Option<Found> {
    let names = config.local_refs.as_ref()?;
    let mut found = Found::new();
    for file in kept {
        let refs = refs::find(&file.text, names);
        if !refs.is_empty() {
            found.insert(file.path.clone(), refs);
        }
    }
    Some(found)
}

/// What `docfit check`, `ratchet` and `init` read of a repo beyond its
/// documents: its text files `docfit.toml` does not ignore, the local refs in
/// them, and its string catalogs.
struct Read {
    kept: Vec<Text>,
    found: Option<Found>,
    catalogs: Vec<Text>,
}

impl Read {
    fn at(top: &Path, config: &Config, tracked: &[String]) -> Result<Read, Vec<Refusal>> {
        let kept = texts(top, config.kept(tracked));
        Ok(Read {
            found: found(config, &kept),
            catalogs: catalogs(top, config, tracked)?,
            kept,
        })
    }

    /// What each document `answer` registers restates, against the other
    /// text files and the catalogs.
    fn restated(&self, answer: &Answer) -> BTreeMap<&str, Restates> {
        let index = Index::new(&self.kept);
        let documents = self
            .kept
            .iter()
            .filter(|t| answer.documents.contains_key(&t.path));
        documents
            .map(|t| {
                (
                    t.path.as_str(),
                    restates::find(
                        &t.path,
                        &t.text,
                        &paragraph::split(&t.text),
                        &self.catalogs,
                        &index,
                    ),
                )
            })
            .collect()
    }
}

/// The baseline `held` ratchets to: the budgets', the local refs' and the
/// restatements'.
fn ratcheted(
    docs: &[Measured],
    strays: &[Stray],
    found: Option<&Found>,
    restated: &BTreeMap<&str, Restates>,
    held: Option<&Baseline>,
) -> Baseline {
    let mut next = budget::ratchet(docs, strays, held);
    refs::ratchet(found, held, &mut next);
    restates::ratchet(restated, held, &mut next);
    next
}

/// The documents registered at `top`, the paths git tracks, and `docfit.toml` read.
fn registered_at(top: &Path) -> Result<(Answer, Vec<String>, Config), Vec<Refusal>> {
    let config = config(top)?.ok_or_else(|| {
        vec![refusal(
            &format!("there is no {CONFIG} at the repo's top"),
            &format!("run `docfit init`, or write {CONFIG} with a `[{TABLE}]` table, a line `\"<glob>\" = \"<kind>\"` per document (see `docfit check --help`)"),
        )]
    })?;
    let config = documents::config(&config)?;
    let tracked = repo::tracked(top)?;
    let answer = documents::check(&config, &tracked);
    Ok((answer, tracked, config))
}

/// Every registered document, with its text read from the working tree.
fn read<'a>(
    top: &Path,
    answer: &'a Answer,
) -> Result<Vec<(&'a str, Document, String)>, Vec<Refusal>> {
    let (mut texts, mut refusals) = (Vec::new(), Vec::new());
    for (file, doc) in &answer.documents {
        match tracked_text(top, file) {
            Ok(text) => texts.push((file.as_str(), *doc, text)),
            Err(r) => refusals.push(r),
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
/// `budget::check` finds in `docs` and `strays` against `baseline`, those
/// `refs::check` finds in `found`, and those `restates::check` finds in
/// `restated`.
fn faults(
    answer: &Answer,
    docs: &[Measured],
    strays: &[Stray],
    (found, restated): (Option<&Found>, &BTreeMap<&str, Restates>),
    baseline: Option<&Baseline>,
) -> Vec<Value> {
    let mut faults: Vec<Value> = answer
        .faults
        .iter()
        .filter(|f| f["fault"] != "unregistered")
        .cloned()
        .collect();
    faults.extend(budget::check(docs, strays, baseline));
    faults.extend(found.map_or_else(Vec::new, |f| refs::check(f, baseline)));
    faults.extend(restates::check(restated, baseline));
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
    let top = top(repo)?;
    let (answer, tracked, config) = registered_at(&top)?;
    let seen = Read::at(&top, &config, &tracked)?;
    let restated = seen.restated(&answer);
    let texts = read(&top, &answer)?;
    let docs = measured(&texts, &Tree::new(&tracked));
    let strays = strays(&top, &answer)?;
    let held = baseline(&top)?;
    let found = seen.found.as_ref();
    let faults = faults(&answer, &docs, &strays, (found, &restated), held.as_ref());
    let tighter = held
        .as_ref()
        .is_some_and(|h| &ratcheted(&docs, &strays, found, &restated, Some(h)) != h);
    let off = if found.is_none() {
        format!(
            "; the `local_ref` check is off: to turn it on, write `{} = [\"<name>\", ...]` in {CONFIG}, the names whose `<name>#<digits>` cites the local tracker (`docfit check --help`)",
            refs::KEY
        )
    } else {
        String::new()
    };
    let (code, next) = match faults.first() {
        Some(first) => (EXIT_FAULTS, first["fix"].clone()),
        None if tighter => (
            EXIT_OK,
            json!(format!(
                "run `docfit ratchet`: a file shrank or went, and {BASELINE} can hold it tighter{off}"
            )),
        ),
        None => (
            EXIT_OK,
            json!(format!(
                "nothing to do: every Markdown file is registered under one kind, within its budget, or held to rehome{off}"
            )),
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
    let adoption = documents::adopt(config.as_deref(), &files)?;
    let text = adoption.text;
    let adopting = documents::config(&text)?;
    let answer = documents::check(&adopting, &files);
    let texts = read(&top, &answer)?;
    let strays = strays(&top, &answer)?;
    let docs = measured(&texts, &Tree::new(&files));
    let seen = Read::at(&top, &adopting, &files)?;
    let restated = seen.restated(&answer);
    let found = seen.found.as_ref();
    let held = baseline(&top)?;
    let adopted = held.is_none();
    let mut next = match &held {
        Some(held) => held.clone(),
        None => budget::ratchet(&docs, &strays, None),
    };
    // A baseline holding no refs has not adopted the check: hold them now.
    if !refs::adopted(&next) {
        refs::ratchet(found, None, &mut next);
    }
    restates::adopt(&restated, &mut next);
    let wrote_baseline = held.as_ref() != Some(&next);
    if wrote_baseline {
        write(&top.join(BASELINE), &(pretty(&json!(next)) + "\n"))?;
    }
    let wrote_config = config.as_deref() != Some(text.as_str());
    if wrote_config {
        write(&top.join(CONFIG), &text)?;
    }
    let rehome: Vec<Value> = strays
        .iter()
        .filter(|_| adopted)
        .map(|s| json!({ "file": s.file, "size": s.bytes, "kinds": s.fault["kinds"], "fix": s.fault["fix"] }))
        .collect();
    let faults = faults(&answer, &docs, &strays, (found, &restated), Some(&next));
    let changed = wrote_baseline || wrote_config;
    let then = match faults.first() {
        Some(first) => first["fix"].clone(),
        None if changed => json!(format!(
            "commit {CONFIG} and {BASELINE}, then `docfit check`; register a file in `rehome` under a kind where one fits it"
        )),
        None => json!("nothing to adopt: run `docfit check`"),
    };
    let registered: std::collections::BTreeMap<_, _> = adoption
        .registered
        .iter()
        .map(|(f, k)| (f, k.name()))
        .collect();
    let ignored: Vec<Value> = adoption
        .ignored
        .iter()
        .map(|(glob, files)| json!({ "glob": glob, "files": files }))
        .collect();
    Ok((
        pretty(&json!({
            "registered": registered, "ignored": ignored, "rehome": rehome, "faults": faults, "baseline": next,
            "changed": changed, "next": then,
        })),
        EXIT_OK,
    ))
}

fn ratchet(repo: Option<PathBuf>) -> Result<(String, i32), Vec<Refusal>> {
    let top = top(repo)?;
    let (answer, tracked, config) = registered_at(&top)?;
    let seen = Read::at(&top, &config, &tracked)?;
    let restated = seen.restated(&answer);
    let texts = read(&top, &answer)?;
    let docs = measured(&texts, &Tree::new(&tracked));
    let held = baseline(&top)?;
    let strays = strays(&top, &answer)?;
    let next = ratcheted(
        &docs,
        &strays,
        seen.found.as_ref(),
        &restated,
        held.as_ref(),
    );
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

fn next(repo: Option<PathBuf>, all: bool) -> Result<(String, i32), Vec<Refusal>> {
    let top = top(repo)?;
    let seen = Seen::at(&top)?;
    let texts = read(&top, &seen.answer)?;
    let docs = measured(&texts, &Tree::new(&seen.tracked));
    let strays = strays(&top, &seen.answer)?;
    let (held, notes) = match baseline(&top)? {
        Some(held) => (held, Vec::<String>::new()),
        None => (
            Baseline::new(),
            vec![format!(
                "no {BASELINE}: nothing is held, so no debt is listed; `docfit init` adopts docfit and writes it"
            )],
        ),
    };
    let steps = RehomeTopic::MARKDOWN;
    let (mut tasks, mut counts) = (Vec::new(), serde_json::Map::new());
    for (kind, debts) in tasks::debt(&docs, &strays, &held) {
        counts.insert(kind.to_string(), json!(debts.len()));
        for debt in debts {
            let text = tracked_text(&top, debt.file).map_err(|r| vec![r])?;
            let mut evidence = debt.evidence;
            evidence["where"] = seen.place(debt.file, &text);
            tasks.push(json!({
                "kind": kind, "summary": debt.summary, "files": [debt.file],
                "steps": steps,
                "accept": format!(
                    "`docfit ratchet`, then `docfit check` exits {EXIT_OK} and {BASELINE} no longer holds `{}`",
                    debt.file
                ),
                "evidence": evidence,
            }));
        }
    }
    let then = if tasks.is_empty() {
        "nothing to do: the baseline holds no document over its budget and no file to rehome"
    } else {
        "do the first task by its steps, then its accept, and commit; then `docfit next` again"
    };
    let mut answer = json!({
        "by": format!("docfit {}", env!("DOCFIT_VERSION")),
        "counts": counts, "notes": notes, "next": then,
    });
    if all {
        answer["tasks"] = json!(tasks);
    } else {
        answer["task"] = tasks.into_iter().next().unwrap_or(Value::Null);
    }
    Ok((pretty(&answer), EXIT_OK))
}

fn place(file: &Path) -> Result<(String, i32), Vec<Refusal>> {
    let unreadable = |e: std::io::Error| {
        vec![refusal(
            &format!("cannot read `{}`: {e}", file.display()),
            "give the path of a document in a git repository, from the current directory",
        )]
    };
    let path = file.canonicalize().map_err(unreadable)?;
    let text = std::fs::read_to_string(&path).map_err(unreadable)?;
    let top = repo::top(path.parent().expect("a file is in a directory"))?;
    let top = top.canonicalize().map_err(unreadable)?;
    let name = path
        .strip_prefix(&top)
        .expect("git's top holds the file")
        .to_string_lossy()
        .into_owned();
    let seen = Seen::at(&top)?;
    Ok((pretty(&seen.place(&name, &text)), EXIT_OK))
}

/// What `docfit where` reads of a repo, once, to place any of its documents:
/// its registered documents, the paths git tracks, the text of every tracked
/// file outside Markdown, its string catalogs, and its ADRs.
struct Seen {
    answer: Answer,
    tracked: Vec<String>,
    code: Vec<String>,
    read: Read,
    adrs: Vec<Adr>,
}

impl Seen {
    fn at(top: &Path) -> Result<Seen, Vec<Refusal>> {
        let (answer, tracked, config) = registered_at(top)?;
        let code: Vec<String> = texts(top, tracked.iter().filter(|f| !f.ends_with(".md")))
            .into_iter()
            .map(|t| t.text)
            .collect();
        let read = Read::at(top, &config, &tracked)?;
        let adrs = answer
            .documents
            .iter()
            .filter(|(_, d)| d.kind == Kind::Adr)
            .map(|(f, _)| tracked_text(top, f).map(|text| Adr::read(f, &text)))
            .collect::<Result<_, _>>()
            .map_err(|r| vec![r])?;
        Ok(Seen {
            answer,
            tracked,
            code,
            read,
            adrs,
        })
    }

    /// `docfit where`'s answer on the document `name`, from the repo's top, of `text`.
    fn place(&self, name: &str, text: &str) -> Value {
        let paragraphs = paragraph::split(text);
        let (paths, others): (BTreeSet<String>, BTreeSet<String>) =
            paragraph::candidates(&paragraphs)
                .into_iter()
                .partition(|s| paragraph::is_path(s, &self.tracked));
        let index = Index::new(&self.read.kept);
        let restates = restates::find(name, text, &paragraphs, &self.read.catalogs, &index);
        let known = Known {
            words: paragraph::words(others.iter().map(String::as_str), &self.code),
            paths,
            restates: &restates,
            adrs: &self.adrs,
        };
        let placed: Vec<_> = paragraphs
            .iter()
            .map(|p| paragraph::place(p, &known))
            .collect();
        let (mut homes, mut each) = (serde_json::Map::new(), Vec::new());
        for home in Home::ALL {
            let n = placed.iter().filter(|p| p.home == home).count();
            homes.insert(home.name().to_string(), json!(n));
            each.push(format!("{} {n}", home.name()));
        }
        let next = if placed.is_empty() {
            format!("nothing to place: `{name}` has no paragraph")
        } else {
            format!(
                "move each paragraph to its home ({}; `docfit where --help` says what each is), then run `docfit check`",
                each.join(", ")
            )
        };
        let kind = self.answer.documents.get(name).map(|d| d.kind.name());
        json!({ "file": name, "kind": kind, "paragraphs": placed, "homes": homes, "next": next })
    }
}

/// Each tracked file `catalogs` matches, with its text; every glob must match one.
fn catalogs(top: &Path, config: &Config, tracked: &[String]) -> Result<Vec<Text>, Vec<Refusal>> {
    let (mut texts, mut refusals) = (Vec::new(), Vec::new());
    for glob in &config.catalogs {
        let files: Vec<&String> = tracked.iter().filter(|f| glob.is_match(f)).collect();
        if files.is_empty() {
            let glob = glob.glob().glob();
            refusals.push(refusal(
                &format!("`{glob}` in {CONFIG}'s `{CATALOGS}` matches no file git tracks"),
                &format!("fix `{glob}` to match the repo's string catalogs, or take it out of `{CATALOGS}`"),
            ));
        }
        for file in files {
            match tracked_text(top, file) {
                Ok(text) => texts.push(Text {
                    path: file.clone(),
                    text,
                }),
                Err(r) => refusals.push(r),
            }
        }
    }
    if refusals.is_empty() {
        Ok(texts)
    } else {
        Err(refusals)
    }
}

/// A tracked file's text.
fn tracked_text(top: &Path, file: &str) -> Result<String, Refusal> {
    std::fs::read_to_string(top.join(file)).map_err(|e| {
        refusal(
            &format!("cannot read `{file}`, which git tracks: {e}"),
            &format!("restore `{file}` as UTF-8 text, or `git rm` it"),
        )
    })
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

/// `names`, each in backticks, comma-separated.
fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
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
