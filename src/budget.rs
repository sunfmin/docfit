//! Decides budgets: a document's size and longest paragraph, against its budget
//! and the baseline (`.docfit/baseline.json`), which only tightens; a
//! document's broken pointers, against those the baseline holds from
//! adoption; and a file no kind fits, held to rehome at its size. Pure: the
//! files' sizes, broken pointers and the baseline arrive as arguments.

use crate::paragraph;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The baseline, from the repo's top.
pub(crate) const BASELINE: &str = ".docfit/baseline.json";

/// What a document is measured by: its size in bytes, and its longest
/// paragraph in characters. A budget and a baseline's hold are each one of
/// these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Size {
    pub(crate) bytes: usize,
    pub(crate) paragraph: usize,
}

impl Size {
    /// `text` measured; with it, the line (from 1) its longest paragraph
    /// starts on.
    pub(crate) fn of(text: &str) -> (Size, usize) {
        let (paragraph, at) = paragraph::longest(text);
        (
            Size {
                bytes: text.len(),
                paragraph,
            },
            at,
        )
    }
}

/// What the baseline holds a document at: each measure it is over its budget
/// by, at the most it may be, and each pointer it held broken at adoption,
/// not fixed since; a measure within budget is not held.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hold {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) paragraph: Option<usize>,
    /// The longest line a baseline written before paragraphs held: read,
    /// never written, so a hold with it differs from what `ratchet` writes.
    #[serde(default, skip_serializing)]
    pub(crate) line: Option<usize>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(crate) pointers: BTreeSet<String>,
    /// The local tracker refs it is held at (`refs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) local_refs: Option<usize>,
}

/// What the baseline holds a file at: a document over its budget, or a file
/// registered under no kind, to rehome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The local tracker refs it is held at (`refs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) local_refs: Option<usize>,
}

/// The baseline: each file it holds, at the size it may not grow past.
pub(crate) type Baseline = BTreeMap<String, Held>;

/// What `baseline` holds the document `file` at over its budget.
fn hold(baseline: &Baseline, file: &str) -> Hold {
    match baseline.get(file) {
        Some(Held::Over(hold)) => hold.clone(),
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

/// One document to judge: its path, its budget, its size, the line (from 1)
/// its longest paragraph starts on, and the `broken_pointer` faults on it, each
/// naming its `pointer`.
pub(crate) struct Measured<'a> {
    pub(crate) file: &'a str,
    pub(crate) budget: Size,
    pub(crate) size: Size,
    pub(crate) at: usize,
    pub(crate) broken: Vec<Value>,
}

/// The pointer a `broken_pointer` fault names.
fn pointer(fault: &Value) -> &str {
    fault["pointer"]
        .as_str()
        .expect("a broken pointer names its pointer")
}

/// The faults of `docs` and `strays`: a stray the baseline holds to rehome is
/// one only once it grows past that, and a pointer the baseline holds broken
/// is none. With no baseline yet, each size fix adds the ratchet that holds
/// every file at its size now.
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
        faults.extend(
            doc.broken
                .iter()
                .filter(|f| !held.pointers.contains(pointer(f)))
                .cloned(),
        );
        let limit = Size {
            bytes: held.bytes.unwrap_or(0).max(doc.budget.bytes),
            paragraph: held.paragraph.unwrap_or(0).max(doc.budget.paragraph),
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
        if size.paragraph > limit.paragraph {
            faults.push(json!({
                "file": doc.file, "fault": "paragraph_too_long", "line": at, "size": size.paragraph, "limit": limit.paragraph,
                "fix": format!(
                    "split the paragraph at line {at} of `{}` into paragraphs of at most {} characters each (it is {}{}), or move part of it to its home, which `docfit where {}` suggests{adopt}",
                    doc.file, limit.paragraph, size.paragraph, grew(doc.budget.paragraph, limit.paragraph), doc.file
                ),
            }));
        }
    }
    for stray in strays {
        match baseline.and_then(|b| b.get(stray.file)) {
            Some(Held::Rehome(Rehome { rehome, .. })) => {
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
/// to rehome once it is no stray: gone, or registered under a kind. Broken
/// pointers go the same way: with no baseline yet each one now is held, after
/// that one held leaves once it is fixed, and none is added.
pub(crate) fn ratchet(
    docs: &[Measured],
    strays: &[Stray],
    baseline: Option<&Baseline>,
) -> Baseline {
    let mut next = Baseline::new();
    for stray in strays {
        let held = match baseline.map(|b| b.get(stray.file)) {
            None => Some(stray.bytes),
            Some(Some(Held::Rehome(Rehome { rehome, .. }))) => Some(stray.bytes.min(*rehome)),
            Some(_) => None,
        };
        if let Some(rehome) = held {
            next.insert(
                stray.file.to_string(),
                Held::Rehome(Rehome {
                    rehome,
                    local_refs: None,
                }),
            );
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
        let broken = doc.broken.iter().map(|f| pointer(f).to_string());
        let pointers = match &held {
            None => broken.collect(),
            Some(h) => broken.filter(|p| h.pointers.contains(p)).collect(),
        };
        let hold = Hold {
            bytes: hold(now.bytes, held.as_ref().map(|h| h.bytes), doc.budget.bytes),
            paragraph: hold(
                now.paragraph,
                held.as_ref().map(|h| h.paragraph),
                doc.budget.paragraph,
            ),
            line: None,
            pointers,
            local_refs: None,
        };
        if hold != Hold::default() {
            next.insert(doc.file.to_string(), Held::Over(hold));
        }
    }
    next
}
