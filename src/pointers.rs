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

/// The tree: every path git tracks, and so every directory holding one.
pub(crate) struct Tree(BTreeSet<String>);

impl Tree {
    pub(crate) fn new(tracked: &[String]) -> Tree {
        Tree(tracked.iter().cloned().collect())
    }

    /// Whether a tracked path starts with `prefix`.
    fn under(&self, prefix: &str) -> bool {
        let first = self.0.range(prefix.to_string()..).next();
        first.is_some_and(|p| p.starts_with(prefix))
    }

    /// Whether `path`, a file or a directory, is in the tree.
    fn holds(&self, path: &str) -> bool {
        let file = path.trim_end_matches('/');
        self.0.contains(file) || self.under(&format!("{file}/"))
    }
}

/// The faults of `file`'s `text`: each pointer to what is not in `tree`, in
/// line order. A fenced code block points at nothing.
pub(crate) fn check(file: &str, text: &str, tree: &Tree) -> Vec<Value> {
    let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
    let mut faults = Vec::new();
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
        let mut missing = |pointer: &str, path: String, holds: bool| {
            if !holds {
                faults.push(fault(file, n + 1, pointer, &path));
            }
        };
        for (i, span) in line.split('`').enumerate() {
            if i % 2 == 1 {
                if let Some(path) = repo_path(span) {
                    let (top, here) = (resolve("", path), resolve(dir, path));
                    let holds = [&top, &here]
                        .iter()
                        .any(|p| p.as_ref().is_some_and(|p| tree.holds(p)));
                    missing(span, top.unwrap_or_else(|| path.to_string()), holds);
                }
                continue;
            }
            for target in links(span) {
                let path = resolve(dir, target);
                let holds = path.as_ref().is_some_and(|p| tree.holds(p));
                missing(target, path.unwrap_or_else(|| target.to_string()), holds);
            }
            for (named, number) in adrs(span) {
                let prefix = format!("{ADRS}{number:0>4}-");
                missing(named, format!("{prefix}*"), tree.under(&prefix));
            }
        }
    }
    faults
}

fn fault(file: &str, line: usize, pointer: &str, missing: &str) -> Value {
    json!({
        "file": file, "fault": "broken_pointer", "line": line, "pointer": pointer, "missing": missing,
        "fix": format!(
            "line {line} of `{file}` points at `{missing}`, which git does not track: point it at what is there now, or delete it; a name that is not a repo path goes without backticks"
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
    let (stem, extension) = path.rsplit_once('.').unwrap_or((path, ""));
    let path_like = path.contains('/')
        || (!stem.is_empty() && !stem.ends_with('/') && EXTENSIONS.contains(&extension));
    (path_like && path.chars().all(named) && !path.starts_with(['/', '-'])).then_some(path)
}

/// The local targets of the Markdown links in `text`: `[..](target)`, without
/// a title or `#anchor`; a URL or an anchor alone is not one.
fn links(text: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    for (_, rest) in text.match_indices("](").map(|(i, _)| text.split_at(i + 2)) {
        let target = rest.split(')').next().unwrap_or("");
        let target = target.trim().trim_start_matches('<');
        let target = target.split(['>', ' ', '#', '?']).next().unwrap_or("");
        let url = target.split('/').next().is_some_and(|s| s.contains(':'));
        if !target.is_empty() && !url {
            targets.push(target);
        }
    }
    targets
}

/// Each ADR `text` names by number, `ADR 0007` or `ADR-0007`: as written, and
/// its number.
fn adrs(text: &str) -> Vec<(&str, &str)> {
    let mut named = Vec::new();
    for (i, _) in text.match_indices("ADR") {
        let before = text[..i].chars().next_back();
        let rest = &text[i + 3..];
        let digits = rest.get(1..).unwrap_or("");
        let len = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        let after = digits[len..].chars().next();
        if rest.starts_with([' ', '-'])
            && len > 0
            && !before.is_some_and(char::is_alphanumeric)
            && !after.is_some_and(char::is_alphanumeric)
        {
            named.push((&text[i..i + 4 + len], &digits[..len]));
        }
    }
    named
}

/// `path` from directory `dir`, both from the repo's top, with `.` and `..`
/// walked; `None` when it climbs out of the repo. A leading `/` is the top.
fn resolve(dir: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    Some(if path.ends_with('/') {
        joined + "/"
    } else {
        joined
    })
}
