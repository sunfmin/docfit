//! Decides pointers: each repo path a document names in backticks or links
//! to, and each ADR it names by number, against the paths git tracks. Pure:
//! the document's text and the tracked paths arrive as arguments.

use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Where a repo keeps its ADRs, from its top: `NNNN-<title>.md`.
pub(crate) const ADRS: &str = "docs/adr/";

/// What a name without a `/` ends in to be read as a path; a file kind
/// outside these is a path only with a `/`.
const EXTENSIONS: [&str; 14] = [
    "md", "toml", "json", "yaml", "yml", "rs", "sh", "txt", "lock", "py", "js", "ts", "go", "html",
];

/// The extensions, for the manual: `.md`, `.toml`, ...
pub(crate) fn extensions() -> String {
    let dotted: Vec<_> = EXTENSIONS.iter().map(|e| format!("`.{e}`")).collect();
    dotted.join(", ")
}

/// The tree: every path git tracks, and so every directory holding one, and
/// the paths git ignores among those a document points at.
pub(crate) struct Tree {
    tracked: BTreeSet<String>,
    ignored: BTreeSet<String>,
}

impl Tree {
    pub(crate) fn new(tracked: &[String]) -> Tree {
        Tree {
            tracked: tracked.iter().cloned().collect(),
            ignored: BTreeSet::new(),
        }
    }

    /// The tree, with `ignored` the paths git ignores: a generated path, so
    /// a pointer to one is no fault.
    pub(crate) fn ignoring(self, ignored: impl IntoIterator<Item = String>) -> Tree {
        Tree {
            ignored: ignored.into_iter().collect(),
            ..self
        }
    }

    /// Whether a tracked path starts with `prefix`.
    fn under(&self, prefix: &str) -> bool {
        let first = self.tracked.range(prefix.to_string()..).next();
        first.is_some_and(|p| p.starts_with(prefix))
    }

    /// Whether `path`, a file or a directory, is in the tree.
    fn holds(&self, path: &str) -> bool {
        let file = path.trim_end_matches('/');
        self.tracked.contains(file) || self.under(&format!("{file}/"))
    }
}

/// A pointer to what the tree does not hold: its line, as written, what it
/// is missing, and each path it is read as, from the repo's top.
struct Missing {
    line: usize,
    pointer: String,
    missing: String,
    paths: Vec<String>,
}

/// The faults of `file`'s `text`: each pointer to what is not in `tree`, and
/// is no path it ignores, in line order. A fenced code block points at nothing.
pub(crate) fn check(file: &str, text: &str, tree: &Tree) -> Vec<Value> {
    missing(file, text, tree)
        .into_iter()
        .filter(|m| !m.paths.iter().any(|p| tree.ignored.contains(p)))
        .map(|m| fault(file, m.line, &m.pointer, &m.missing))
        .collect()
}

/// Each path, from the repo's top, that a pointer of `file`'s `text` to what
/// is not in `tree` is read as: what to ask git whether it ignores.
pub(crate) fn unheld(file: &str, text: &str, tree: &Tree) -> Vec<String> {
    missing(file, text, tree)
        .into_iter()
        .flat_map(|m| m.paths)
        .collect()
}

/// Each pointer of `file`'s `text` to what is not in `tree`, in line order.
fn missing(file: &str, text: &str, tree: &Tree) -> Vec<Missing> {
    let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
    let mut found = Vec::new();
    let mut fenced = false;
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut missing = |pointer: &str, missing: String, paths: Vec<Option<String>>| {
            if !paths.iter().flatten().any(|p| tree.holds(p)) {
                found.push(Missing {
                    line: n + 1,
                    pointer: pointer.to_string(),
                    missing,
                    paths: paths.into_iter().flatten().collect(),
                });
            }
        };
        for (i, span) in line.split('`').enumerate() {
            if i % 2 == 1 {
                if let Some(path) = repo_path(span) {
                    let root = first_segment(path);
                    let rooted = |base| resolve(base, root).is_some_and(|r| tree.holds(&r));
                    if !named_file(path) && !rooted("") && !rooted(dir) {
                        continue;
                    }
                    let (top, here) = (resolve("", path), resolve(dir, path));
                    let named = top.clone().unwrap_or_else(|| path.to_string());
                    missing(span, named, vec![top, here]);
                }
                continue;
            }
            for target in links(span) {
                let path = resolve(dir, target);
                let named = path.clone().unwrap_or_else(|| target.to_string());
                missing(target, named, vec![path]);
            }
            for (named, number) in adrs(span) {
                let prefix = format!("{ADRS}{number:0>4}-");
                if !tree.under(&prefix) {
                    missing(named, format!("{prefix}*"), Vec::new());
                }
            }
        }
    }
    found
}

fn fault(file: &str, line: usize, pointer: &str, missing: &str) -> Value {
    json!({
        "file": file, "fault": "broken_pointer", "line": line, "pointer": pointer, "missing": missing,
        "fix": format!(
            "line {line} of `{file}` points at `{missing}`, which git neither tracks nor ignores: point it at what is there now, or delete it; a name that is not a repo path goes without backticks"
        ),
    })
}

/// The repo path a backticked `span` names, without a `:<line>` after it;
/// `None` when it is not one: a command, a glob, a placeholder, a URL, an
/// absolute or home path, or a name without a `/` and a file's extension.
fn repo_path(span: &str) -> Option<&str> {
    let path = match span.rsplit_once(':') {
        Some((path, line)) if !line.is_empty() && line.chars().all(|c| c.is_ascii_digit()) => path,
        _ => span,
    };
    let named = |c: char| c.is_alphanumeric() || "._-/+".contains(c);
    let path_like = path.contains('/') || named_file(path);
    (path_like && path.chars().all(named) && !path.starts_with(['/', '-'])).then_some(path)
}

/// Whether `path` names a file by one of the [`EXTENSIONS`]: such a name is a
/// pointer wherever it is, and any other is one only from a [`first_segment`]
/// git tracks.
fn named_file(path: &str) -> bool {
    let (stem, extension) = path.rsplit_once('.').unwrap_or((path, ""));
    !stem.is_empty() && !stem.ends_with('/') && EXTENSIONS.contains(&extension)
}

/// `path` up to its first segment that names something, past any `.` or
/// `..`: `src` of `src/a/b`, `../adr` of `../adr/0002`.
fn first_segment(path: &str) -> &str {
    let mut end = 0;
    for part in path.split('/') {
        end += part.len();
        if !matches!(part, "" | "." | "..") {
            break;
        }
        end += 1;
    }
    &path[..end.min(path.len())]
}

/// The local targets of the Markdown links in `text`: `[..](target)`, without
/// a title or `#anchor`; a URL or an anchor alone is not one.
fn links(text: &str) -> impl Iterator<Item = &str> {
    text.split("](").skip(1).filter_map(|rest| {
        let target = rest.split(')').next()?.trim().trim_start_matches('<');
        let target = target.split(['>', ' ', '#', '?']).next()?;
        let url = target.split('/').next()?.contains(':');
        (!target.is_empty() && !url).then_some(target)
    })
}

/// Each ADR `text` names by number, `ADR 0007` or `ADR-0007`: as written, and
/// its number.
fn adrs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.match_indices("ADR").filter_map(|(i, _)| {
        let rest = text[i + 3..].strip_prefix([' ', '-'])?;
        let len = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let alnum = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
        let bounded = !alnum(text[..i].chars().next_back()) && !alnum(rest[len..].chars().next());
        (len > 0 && bounded).then(|| (&text[i..i + 4 + len], &rest[..len]))
    })
}

/// `path` from directory `dir`, both from the repo's top, with `.` and `..`
/// walked; `None` when it climbs out of the repo. A leading `/` is the top.
fn resolve(dir: &str, path: &str) -> Option<String> {
    let dir = if path.starts_with('/') { "" } else { dir };
    let mut parts = Vec::new();
    for part in dir.split('/').chain(path.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let slash = if path.ends_with('/') { "/" } else { "" };
    Some(parts.join("/") + slash)
}
