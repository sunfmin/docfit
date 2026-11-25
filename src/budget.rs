//! Decides budgets: a document's size and longest line, against its budget
//! and the baseline (`.docfit/baseline.json`), which only tightens; and a file
//! no kind fits, held to rehome at its size. Pure: the files' sizes and the
//! baseline arrive as arguments.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The baseline, from the repo's top.
pub(crate) const BASELINE: &str = ".docfit/baseline.json";

/// What a document is measured by: its size in bytes, and its longest line in
/// characters. A budget and a baseline's hold are each one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Size {
    pub(crate) bytes: usize,
    pub(crate) line: usize,
}

impl Size {
    /// `text` measured; with it, the number (from 1) of its longest line.
    pub(crate) fn of(text: &str) -> (Size, usize) {
        let (mut line, mut at) = (0, 0);
        for (n, l) in text.lines().enumerate() {
            let chars = l.chars().count();
            if chars > line {
                (line, at) = (chars, n + 1);
            }
        }
        (
            Size {
                bytes: text.len(),
                line,
            },
            at,
        )
    }
}

/// What the baseline holds a document at: each measure it is over its budget
/// by, at the most it may be; a measure within budget is not held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hold {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<usize>,
}

/// What the baseline holds a file at: a document over its budget, or a file
/// registered under no kind, to rehome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Held {
    Rehome(Rehome),
    Over(Hold),
}

/// A file no kind fits, to rehome: the size in bytes it may not grow past,
/// until its content is moved to the documents whose kinds it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rehome {
    pub(crate) rehome: usize,
}

/// The baseline: each file it holds, at the size it may not grow past.
pub(crate) type Baseline = BTreeMap<String, Held>;

/// What `baseline` holds the document `file` at over its budget.
fn hold(baseline: &Baseline, file: &str) -> Hold {
    match baseline.get(file) {
        Some(Held::Over(hold)) => *hold,
        _ => Hold::default(),
    }
}

/// A tracked Markdown file registered under no kind: its path, its size in
/// bytes, and the `unregistered` fault `docfit check` answers on it.
pub(crate) struct Stray<'a> {
    pub(crate) file: &'a str,
    pub(crate) bytes: usize,
    pub(crate) fault: &'a Value,
}

/// One document to judge: its path, its budget, its size, and the number
/// (from 1) of its longest line.
pub(crate) struct Measured<'a> {
    pub(crate) file: &'a str,
    pub(crate) budget: Size,
    pub(crate) size: Size,
    pub(crate) at: usize,
}

/// The faults of `docs` and `strays`: a stray the baseline holds to rehome is
/// one only once it grows past that. With no baseline yet, each fix adds the
/// ratchet that holds every file at its size now.
pub(crate) fn check(
    docs: &[Measured],
    strays: &[Stray],
    baseline: Option<&Baseline>,
) -> Vec<Value> {
    let adopt = if baseline.is_none() {
        format!(
            "; or, adopting docfit, `docfit ratchet` writes {BASELINE}, holding every document at its size now"
        )
    } else {
        String::new()
    };
    let mut faults = Vec::new();
    for doc in docs {
        let held = baseline.map(|b| hold(b, doc.file)).unwrap_or_default();
        let limit = Size {
            bytes: held.bytes.unwrap_or(0).max(doc.budget.bytes),
            line: held.line.unwrap_or(0).max(doc.budget.line),
        };
        let (size, at) = (doc.size, doc.at);
        let grew = |budget: usize, limit: usize| {
            if limit > budget {
                format!(", past the {limit} {BASELINE} holds it at")
            } else {
                String::new()
            }
        };
        if size.bytes > limit.bytes {
            faults.push(json!({
                "file": doc.file, "fault": "over_budget", "size": size.bytes, "limit": limit.bytes,
                "fix": format!(
                    "cut `{}` to at most {} bytes (it is {}{}): move what is not its kind's to the document whose kind it is, or delete it{adopt}",
                    doc.file, limit.bytes, size.bytes, grew(doc.budget.bytes, limit.bytes)
                ),
            }));
        }
        if size.line > limit.line {
            faults.push(json!({
                "file": doc.file, "fault": "line_too_long", "line": at, "size": size.line, "limit": limit.line,
                "fix": format!(
                    "break line {at} of `{}` at its sentences, one per line, each at most {} characters (it is {}{}): `docfit fmt {}` breaks it at its sentences, and a sentence still too long is cut by hand{adopt}",
                    doc.file, limit.line, size.line, grew(doc.budget.line, limit.line), doc.file
                ),
            }));
        }
    }
    for stray in strays {
        match baseline.and_then(|b| b.get(stray.file)) {
            Some(Held::Rehome(Rehome { rehome })) => {
                if stray.bytes > *rehome {
                    faults.push(json!({
                        "file": stray.file, "fault": "rehome_grew", "size": stray.bytes, "limit": rehome,
                        "fix": format!(
                            "cut `{}` back to at most {rehome} bytes (it is {}), the size {BASELINE} holds it at to rehome: move its content to the documents whose kinds it holds, or register it under a kind",
                            stray.file, stray.bytes
                        ),
                    }));
                }
            }
            _ => faults.push(stray.fault.clone()),
        }
    }
    faults
}

/// The baseline moved down to the sizes of `docs` and `strays`, never up.
/// With no baseline yet, it holds each document over its budget at its size
/// now, and each stray to rehome at its size now; after that, a measure it
/// does not hold is held at the budget, so is never added, and a stray it
/// does not hold to rehome is not added. A measure back within its budget
/// leaves it, a document with none held leaves it too, and so does a file held
/// to rehome once it is no stray: gone, or registered under a kind.
pub(crate) fn ratchet(
    docs: &[Measured],
    strays: &[Stray],
    baseline: Option<&Baseline>,
) -> Baseline {
    let mut next = Baseline::new();
    for stray in strays {
        let held = match baseline.map(|b| b.get(stray.file)) {
            None => Some(stray.bytes),
            Some(Some(Held::Rehome(Rehome { rehome }))) => Some(stray.bytes.min(*rehome)),
            Some(_) => None,
        };
        if let Some(rehome) = held {
            next.insert(stray.file.to_string(), Held::Rehome(Rehome { rehome }));
        }
    }
    for doc in docs {
        let now = doc.size;
        let held = baseline.map(|b| hold(b, doc.file));
        let hold = |now: usize, held: Option<Option<usize>>, budget: usize| {
            let at = match held {
                None => now,
                Some(held) => held.map_or(0, |h| h.min(now)),
            };
            (at > budget).then_some(at)
        };
        let hold = Hold {
            bytes: hold(now.bytes, held.map(|h| h.bytes), doc.budget.bytes),
            line: hold(now.line, held.map(|h| h.line), doc.budget.line),
        };
        if hold != Hold::default() {
            next.insert(doc.file.to_string(), Held::Over(hold));
        }
    }
    next
}
