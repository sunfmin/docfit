//! Decides `measurement`: the numbers with a unit in each document of a kind
//! [`Kind::UNMEASURED`] lists, as `docfit where` finds its evidence, against
//! the count the baseline holds each document at, which only shrinks. Pure:
//! the documents' paragraphs and the baseline arrive as arguments.

use crate::budget::{self, Baseline, Slot};
use crate::kind::Kind;
use crate::paragraph::{self, Paragraph};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Each document checked, with its numbers with a unit, each with the line
/// (from 1) its paragraph starts on, in order.
pub(crate) type Found<'a> = BTreeMap<&'a str, Vec<(usize, String)>>;

/// The field of a hold that holds a document's count.
const SLOT: Slot = |h| &mut h.measurements;

/// The numbers with a unit in `paragraphs`, of a document of `kind`; `None`
/// for a kind that may hold them.
pub(crate) fn find(kind: Kind, paragraphs: &[Paragraph]) -> Option<Vec<(usize, String)>> {
    Kind::UNMEASURED
        .contains(&kind)
        .then(|| paragraph::evidence(paragraphs))
}

/// Each document of `found`, with its count now.
fn counts<'a>(found: &'a Found) -> impl Iterator<Item = (&'a str, usize)> {
    found.iter().map(|(f, all)| (*f, all.len()))
}

/// The `measurement` faults of `found`: each document with more than
/// `baseline` holds it at, naming the first past that count.
pub(crate) fn check(found: &Found, baseline: Option<&Baseline>) -> Vec<Value> {
    let mut faults = Vec::new();
    for (file, all) in found {
        let limit = baseline.map_or(0, |b| budget::held(b, file, SLOT));
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

/// `next` holding each document's count, shrink-only
/// ([`budget::ratchet_counts`]).
pub(crate) fn ratchet(found: &Found, held: Option<&Baseline>, next: &mut Baseline) {
    budget::ratchet_counts(counts(found), SLOT, held, next);
}

/// `next` holding each document's count now, when it holds none yet
/// ([`budget::adopt_counts`]).
pub(crate) fn adopt(found: &Found, next: &mut Baseline) {
    budget::adopt_counts(counts(found), SLOT, next);
}
