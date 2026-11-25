//! The kinds of document, fixed in docfit: each kind's name, reader and what
//! it holds, and its default budget. This enum is their one home; the
//! manual's table is made from it.

use crate::budget::Size;

/// A kind of document a repo registers its Markdown files under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Readme,
    Agent,
    Adr,
    Glossary,
    DesignNote,
}

impl Kind {
    pub(crate) const ALL: [Kind; 5] = [
        Kind::Readme,
        Kind::Agent,
        Kind::Adr,
        Kind::Glossary,
        Kind::DesignNote,
    ];

    /// The kind's name, as `docfit.toml` and the answers spell it; who reads
    /// a document of it; and what that document holds, nothing else.
    fn facts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Kind::Readme => (
                "readme",
                "someone new to the repo",
                "what it is, what it keeps where, how to build and run it",
            ),
            Kind::Agent => (
                "agent",
                "every agent, every turn (`CLAUDE.md`, `AGENTS.md`, `docs/agents/*`)",
                "navigation pointers, invariants the code cannot show, unwritten conventions",
            ),
            Kind::Adr => (
                "adr",
                "whoever would undo a decision",
                "one decision and why",
            ),
            Kind::Glossary => (
                "glossary",
                "everyone",
                "the project's words, a line or two each",
            ),
            Kind::DesignNote => (
                "design-note",
                "whoever changes one cross-module mechanism",
                "one mechanism; off unless the repo turns it on",
            ),
        }
    }

    /// The kind's default budget: a document's size in bytes, and its longest
    /// paragraph in characters. A `docfit.toml` entry may set its own.
    pub(crate) fn budget(self) -> Size {
        let kb = match self {
            Kind::Readme => 4,
            Kind::Agent => 2,
            Kind::Adr => 3,
            Kind::Glossary => 8,
            Kind::DesignNote => 6,
        };
        Size {
            bytes: kb * 1024,
            paragraph: 600,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        self.facts().0
    }

    pub(crate) fn named(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The kind a file at `path` is by its place, if its place says one:
    /// `README.md` readme; `CLAUDE.md`, `AGENTS.md` and `docs/agents/` agent;
    /// `GLOSSARY.md` glossary; `adr/` adr.
    pub(crate) fn conventional(path: &str) -> Option<Kind> {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        match file.to_ascii_lowercase().as_str() {
            "readme.md" => Some(Kind::Readme),
            "claude.md" | "agents.md" => Some(Kind::Agent),
            "glossary.md" => Some(Kind::Glossary),
            _ if dir.ends_with("docs/agents") => Some(Kind::Agent),
            _ if dir.ends_with("adr") => Some(Kind::Adr),
            _ => None,
        }
    }

    /// The kinds a file at `path` could be: the one its place conventionally
    /// says, or, where its place says none, every kind.
    pub(crate) fn candidates(path: &str) -> Vec<Kind> {
        Kind::conventional(path).map_or_else(|| Kind::ALL.to_vec(), |k| vec![k])
    }

    /// The kinds as a Markdown table, for the manual.
    pub(crate) fn table() -> String {
        let mut table = String::from(
            "| kind | reader | holds only | bytes | longest paragraph |\n|---|---|---|---|---|\n",
        );
        for kind in Kind::ALL {
            let ((name, reader, holds), budget) = (kind.facts(), kind.budget());
            table.push_str(&format!(
                "| `{name}` | {reader} | {holds} | {} | {} |\n",
                budget.bytes, budget.paragraph
            ));
        }
        table
    }

    /// The kinds' names, comma-separated, for a fix.
    pub(crate) fn names(kinds: &[Kind]) -> String {
        kinds
            .iter()
            .map(|k| k.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
