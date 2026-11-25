//! Decides `docfit check`: which of a repo's tracked Markdown files
//! `docfit.toml` registers, under which kind. Pure: the files and the
//! config's text arrive as arguments.

use crate::budget::Size;
use crate::kind::Kind;
use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The file a repo declares its documents in, at its top.
pub(crate) const CONFIG: &str = "docfit.toml";
/// The table of `docfit.toml` that maps a glob to a kind.
pub(crate) const TABLE: &str = "documents";

/// One fault in `docfit.toml` that stops the check, and its fix.
#[derive(serde::Serialize)]
pub(crate) struct Refusal {
    pub(crate) fault: String,
    pub(crate) fix: String,
}

/// One line of `[documents]`: its glob, its kind, and the measures of the
/// budget it sets itself.
pub(crate) struct Entry {
    matcher: GlobMatcher,
    kind: Kind,
    bytes: Option<usize>,
    line: Option<usize>,
}

/// A registered document: its kind, and its budget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Document {
    pub(crate) kind: Kind,
    pub(crate) budget: Size,
}

/// The check's answer: each document, and each fault with its fix.
pub(crate) struct Answer {
    pub(crate) documents: BTreeMap<String, Document>,
    pub(crate) faults: Vec<Value>,
}

/// An entry's value: a kind's name, or a table that also sets a budget.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Entered {
    Kind(String),
    Budget(Budgeted),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Budgeted {
    kind: String,
    bytes: Option<usize>,
    line: Option<usize>,
}

/// How an entry is written, for a fix.
const SHAPES: &str = "`\"<kind>\"` or `{ kind = \"<kind>\", bytes = <n>, line = <n> }`";

/// `docfit.toml`'s entries.
pub(crate) fn entries(config: &str) -> Result<Vec<Entry>, Vec<Refusal>> {
    let table: toml::Table = config.parse().map_err(|e: toml::de::Error| {
        vec![Refusal {
            fault: format!("{CONFIG} is not TOML: {}", e.message()),
            fix: format!("fix {CONFIG}"),
        }]
    })?;
    let Some(toml::Value::Table(documents)) = table.get(TABLE) else {
        return Err(vec![Refusal {
            fault: format!("{CONFIG} has no `[{TABLE}]` table"),
            fix: format!(
                "add a `[{TABLE}]` table to {CONFIG}, a line `\"<glob>\" = \"<kind>\"` per document"
            ),
        }]);
    };
    let (mut entries, mut refusals) = (Vec::new(), Vec::new());
    for (glob, value) in documents {
        let (kind, bytes, line) = match value.clone().try_into() {
            Ok(Entered::Kind(name)) => (name, None, None),
            Ok(Entered::Budget(b)) => (b.kind, b.bytes, b.line),
            Err(_) => {
                refusals.push(Refusal {
                    fault: format!("`{glob}` = `{value}` is not an entry docfit reads"),
                    fix: format!(
                        "give `{glob}` one of the kinds {}, as {SHAPES}",
                        Kind::names(&Kind::ALL)
                    ),
                });
                continue;
            }
        };
        let kind = Kind::named(&kind).ok_or(kind);
        let matcher = GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map(|g| g.compile_matcher());
        match (kind, matcher) {
            (Ok(kind), Ok(matcher)) => entries.push(Entry {
                matcher,
                kind,
                bytes,
                line,
            }),
            (Err(name), _) => refusals.push(Refusal {
                fault: format!("`{glob}` names the kind `{name}`, which docfit does not know"),
                fix: format!("give `{glob}` one of the kinds {}", Kind::names(&Kind::ALL)),
            }),
            (_, Err(e)) => refusals.push(Refusal {
                fault: format!("`{glob}` is not a glob: {}", e.kind()),
                fix: format!("fix `{glob}` in {CONFIG}"),
            }),
        }
    }
    if refusals.is_empty() {
        Ok(entries)
    } else {
        Err(refusals)
    }
}

/// The check of `files`, the repo's tracked paths, against `entries`.
pub(crate) fn check(entries: &[Entry], files: &[String]) -> Answer {
    let mut answer = Answer {
        documents: BTreeMap::new(),
        faults: Vec::new(),
    };
    for file in files.iter().filter(|f| f.ends_with(".md")) {
        let matching: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.matcher.is_match(file))
            .collect();
        let mut kinds: Vec<Kind> = Vec::new();
        for entry in &matching {
            if !kinds.contains(&entry.kind) {
                kinds.push(entry.kind);
            }
        }
        match kinds.as_slice() {
            [kind] => {
                let set = |measure: fn(&Entry) -> Option<usize>, default: usize| {
                    matching
                        .iter()
                        .filter_map(|e| measure(e))
                        .min()
                        .unwrap_or(default)
                };
                let budget = Size {
                    bytes: set(|e| e.bytes, kind.budget().bytes),
                    line: set(|e| e.line, kind.budget().line),
                };
                answer.documents.insert(
                    file.clone(),
                    Document {
                        kind: *kind,
                        budget,
                    },
                );
            }
            [] => {
                let candidates = Kind::candidates(file);
                let kind = if let [one] = candidates.as_slice() {
                    one.name()
                } else {
                    "<kind>"
                };
                let mut fix =
                    format!("add `\"{file}\" = \"{kind}\"` under `[{TABLE}]` in {CONFIG}");
                if candidates.len() > 1 {
                    fix.push_str(&format!(", <kind> one of {}", Kind::names(&candidates)));
                }
                answer
                    .faults
                    .push(fault(file, "unregistered", &candidates, fix));
            }
            several => {
                let fix = format!(
                    "register `{file}` under one of {} in {CONFIG}, not all",
                    Kind::names(several)
                );
                answer.faults.push(fault(file, "two_kinds", several, fix));
            }
        }
    }
    answer
}

fn fault(file: &str, fault: &str, kinds: &[Kind], fix: String) -> Value {
    json!({ "file": file, "fault": fault, "kinds": kinds.iter().map(|k| k.name()).collect::<Vec<_>>(), "fix": fix })
}
