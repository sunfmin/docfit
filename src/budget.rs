//! Decides budgets: a document's size and longest line, against its budget
//! and the baseline (`.docfit/baseline.json`), which only tightens. Pure: the
//! documents' text sizes and the baseline arrive as arguments.

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

/// The baseline: each document held over its budget, at the size it may not
/// grow past.
pub(crate) type Baseline = BTreeMap<String, Hold>;

/// One document to judge: its path, its budget, its size, and the number
/// (from 1) of its longest line.
pub(crate) struct Measured<'a> {
    pub(crate) file: &'a str,
    pub(crate) budget: Size,
    pub(crate) size: Size,
    pub(crate) at: usize,
}

/// With no baseline yet, each fix adds the ratchet that holds every
/// document at its size now.
pub(crate) fn check(docs: &[Measured], baseline: Option<&Baseline>) -> Vec<Value> {
    let adopt = if baseline.is_none() {
        format!(
            "; or, adopting docfit, `docfit ratchet` writes {BASELINE}, holding every document at its size now"
        )
    } else {
        String::new()
    };
    let mut faults = Vec::new();
    for doc in docs {
        let held = baseline
            .and_then(|b| b.get(doc.file))
            .copied()
            .unwrap_or_default();
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
                    "break line {at} of `{}` at its sentences, one per line, each at most {} characters (it is {}{}); `docfit fmt` (docfit#8) will do it once it exists{adopt}",
                    doc.file, limit.line, size.line, grew(doc.budget.line, limit.line)
                ),
            }));
        }
    }
    faults
}

/// The baseline moved down to `docs`' sizes, never up. With no baseline yet,
/// it holds each document over its budget at its size now; after that, a
/// measure it does not hold is held at the budget, so is never added. A
/// measure back within its budget leaves it, and a document with none held
/// leaves it too.
pub(crate) fn ratchet(docs: &[Measured], baseline: Option<&Baseline>) -> Baseline {
    let mut next = Baseline::new();
    for doc in docs {
        let now = doc.size;
        let held = baseline.map(|b| b.get(doc.file).copied().unwrap_or_default());
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
            next.insert(doc.file.to_string(), hold);
        }
    }
    next
}
