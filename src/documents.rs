//! Decides `docfit check`: which of a repo's tracked Markdown files
//! `docfit.toml` registers, under which kind. Pure: the files and the
//! config's text arrive as arguments.

use crate::kind::Kind;
use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The file a repo declares its documents in, at its top.
pub(crate) const CONFIG: &str = "docfit.toml";
/// The table of `docfit.toml` that maps a glob to a kind.
pub(crate) const TABLE: &str = "documents";

/// One fault in `docfit.toml` that stops the check, and its fix.
pub(crate) struct Refusal {
    pub(crate) fault: String,
    pub(crate) fix: String,
}

/// The check's answer: each document's kind, and each fault with its fix.
pub(crate) struct Answer {
    pub(crate) documents: BTreeMap<String, &'static str>,
    pub(crate) faults: Vec<Value>,
}

/// `docfit.toml`'s entries, glob and kind, in the order written.
pub(crate) fn entries(config: &str) -> Result<Vec<(GlobMatcher, Kind)>, Vec<Refusal>> {
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
    for (glob, kind) in documents {
        let kind = match kind.as_str() {
            Some(name) => Kind::named(name).ok_or(name.to_string()),
            None => Err(kind.to_string()),
        };
        let matcher = GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map(|g| g.compile_matcher());
        match (kind, matcher) {
            (Ok(kind), Ok(matcher)) => entries.push((matcher, kind)),
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
pub(crate) fn check(entries: &[(GlobMatcher, Kind)], files: &[String]) -> Answer {
    let mut answer = Answer {
        documents: BTreeMap::new(),
        faults: Vec::new(),
    };
    for file in files.iter().filter(|f| f.ends_with(".md")) {
        let mut kinds: Vec<Kind> = Vec::new();
        for (matcher, kind) in entries {
            if matcher.is_match(file) && !kinds.contains(kind) {
                kinds.push(*kind);
            }
        }
        match kinds.as_slice() {
            [kind] => {
                answer.documents.insert(file.clone(), kind.name());
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
