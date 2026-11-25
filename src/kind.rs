//! The kinds of document, fixed in docfit: each kind's name, reader and what
//! it holds. This enum is their one home; the manual's table is made from it.

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

    /// The name `docfit.toml` and the answers spell it with.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Readme => "readme",
            Kind::Agent => "agent",
            Kind::Adr => "adr",
            Kind::Glossary => "glossary",
            Kind::DesignNote => "design-note",
        }
    }

    /// Who reads a document of this kind, and what it holds and nothing else.
    fn reader_and_holds(self) -> (&'static str, &'static str) {
        match self {
            Kind::Readme => (
                "someone new to the repo",
                "what it is, what it keeps where, how to build and run it",
            ),
            Kind::Agent => (
                "every agent, every turn (`CLAUDE.md`, `AGENTS.md`, `docs/agents/*`)",
                "navigation pointers, invariants the code cannot show, unwritten conventions",
            ),
            Kind::Adr => ("whoever would undo a decision", "one decision and why"),
            Kind::Glossary => ("everyone", "the project's words, one or two sentences each"),
            Kind::DesignNote => (
                "whoever changes one cross-module mechanism",
                "one mechanism; off unless the repo turns it on",
            ),
        }
    }

    pub(crate) fn named(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The kinds a file at `path` could be: the one its place conventionally
    /// says, or, where its place says none, every kind.
    pub(crate) fn candidates(path: &str) -> Vec<Kind> {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        let file = file.to_ascii_lowercase();
        let conventional = match file.as_str() {
            "readme.md" => Some(Kind::Readme),
            "claude.md" | "agents.md" => Some(Kind::Agent),
            "glossary.md" => Some(Kind::Glossary),
            _ if dir.ends_with("docs/agents") => Some(Kind::Agent),
            _ if dir.ends_with("adr") => Some(Kind::Adr),
            _ => None,
        };
        conventional.map_or_else(|| Kind::ALL.to_vec(), |k| vec![k])
    }

    /// The kinds as a Markdown table, for the manual.
    pub(crate) fn table() -> String {
        let mut table = String::from("| kind | reader | holds only |\n|---|---|---|\n");
        for kind in Kind::ALL {
            let (reader, holds) = kind.reader_and_holds();
            table.push_str(&format!("| `{}` | {reader} | {holds} |\n", kind.name()));
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
