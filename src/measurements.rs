//! Decides `measurement`: the numbers with a unit in each document of a kind
//! [`Kind::UNMEASURED`] lists, as `docfit where` finds its evidence, against
//! the count the baseline holds each document at, which only shrinks. Pure:
//! the documents' paragraphs and the baseline arrive as arguments.

use crate::budget::{Baseline, Held, Hold};
use crate::kind::Kind;
use crate::paragraph::{self, Paragraph};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Each document checked, with its numbers with a unit, each with the line
/// (from 1) its paragraph starts on, in order.
pub(crate) type Found<'a> = BTreeMap<&'a str, Vec<(usize, String)>>;

/// The numbers with a unit in `paragraphs`, of a document of `kind`; `None`
/// for a kind that may hold them.
pub(crate) fn find(kind: Kind, paragraphs: &[Paragraph]) -> Option<Vec<(usize, String)>> {
    Kind::UNMEASURED
        .contains(&kind)
        .then(|| paragraph::evidence(paragraphs))
}

/// The count `baseline` holds `file` at; 0 when it holds none.
fn held(baseline: &Baseline, file: &str) -> usize {
    match baseline.get(file) {
        Some(Held::Over(h)) => h.measurements.unwrap_or(0),
        _ => 0,
    }
}

/// The `measurement` faults of `found`: each document with more than
/// `baseline` holds it at, naming the first past that count.
pub(crate) fn check(found: &Found, baseline: Option<&Baseline>) -> Vec<Value> {
    let mut faults = Vec::new();
    for (file, all) in found {
        let limit = baseline.map_or(0, |b| held(b, file));
        let Some((line, what)) = all.get(limit) else {
            continue;
        };
        faults.push(json!({
            "file": file, "fault": "measurement", "line": line, "found": what,
            "size": all.len(), "limit": limit,
            "fix": format!(
                "`{what}` at line {line} of `{file}` is a measurement, stale on the next change: move it to the ADR of the decision it supports, with the command that measures it, or to the issue that accepted the work; or delete it"
            ),
        }));
    }
    faults
}

/// `next` (the baseline the budgets ratchet to) holding each document's
/// count: with no baseline yet (`held` `None`), at its count now; after
/// that, at the smaller of what `held` holds and its count now, dropped at
/// 0, and never added.
pub(crate) fn ratchet(found: &Found, held: Option<&Baseline>, next: &mut Baseline) {
    for (file, all) in found {
        let count = held.map_or(all.len(), |h| self::held(h, file).min(all.len()));
        if count == 0 {
            continue;
        }
        let entry = next
            .entry(file.to_string())
            .or_insert_with(|| Held::Over(Hold::default()));
        // A document is never a file to rehome, which no kind registers.
        if let Held::Over(h) = entry {
            h.measurements = Some(count);
        }
    }
}

/// `next` holding each document's count now, when it holds no document's
/// count yet: `docfit init`'s adoption, which leaves one already adopted as
/// it is.
pub(crate) fn adopt(found: &Found, next: &mut Baseline) {
    if !next.keys().any(|f| held(next, f) > 0) {
        ratchet(found, None, next);
    }
}
