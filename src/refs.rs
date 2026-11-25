//! Decides `local_ref`: the refs to the local tracker in a repo's text files,
//! `<name>#<digits>` with `<name>` one `local_issue_refs` lists, against the
//! count the baseline holds each file at, which only shrinks. Pure: the
//! files' texts, the names and the baseline arrive as arguments.

use crate::budget::{Baseline, Held, Hold};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The key of `docfit.toml` that lists the names whose `<name>#<digits>`
/// cites the local tracker.
pub(crate) const KEY: &str = "local_issue_refs";

/// A char a name is made of: a ref's name runs back over these.
pub(crate) fn named(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// One local ref: the line (from 1) it is on, and the ref as written.
#[derive(Debug, PartialEq)]
pub(crate) struct Ref {
    pub(crate) line: usize,
    pub(crate) text: String,
}

/// The local refs in `text`, in order: `<name>#<digits>` with `<name>` one of
/// `names`, not run on by a name's char before it or a word's after it, not
/// after a `/` (an owner's, `sunfmin/lore#25`), and not in a URL (a word with
/// `://`).
pub(crate) fn find(text: &str, names: &[String]) -> Vec<Ref> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        for (i, _) in line.match_indices('#') {
            let digits = line[i + 1..].len()
                - line[i + 1..]
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .len();
            let end = i + 1 + digits;
            if digits == 0 || line[end..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let start = line[..i].trim_end_matches(named).len();
            if !names.iter().any(|name| *name == line[start..i]) || line[..start].ends_with('/') {
                continue;
            }
            let word = line[..start]
                .rsplit(|c: char| c.is_whitespace() || "(<[\"'`".contains(c))
                .next()
                .unwrap_or("");
            if !word.contains("://") {
                out.push(Ref {
                    line: n + 1,
                    text: line[start..end].to_string(),
                });
            }
        }
    }
    out
}

/// Each file with a local ref, with its refs in order.
pub(crate) type Found = BTreeMap<String, Vec<Ref>>;

/// The count `held` holds its file's refs at; 0 when it holds none.
fn count(held: &Held) -> usize {
    match held {
        Held::Over(h) => h.local_refs,
        Held::Rehome(r) => r.local_refs,
    }
    .unwrap_or(0)
}

/// The count `baseline` holds `file`'s refs at; 0 when it holds none.
fn held(baseline: &Baseline, file: &str) -> usize {
    baseline.get(file).map_or(0, count)
}

/// `next` holding `file`'s refs at `count`, beside what else it holds it at.
fn hold(next: &mut Baseline, file: &str, count: usize) {
    let entry = next
        .entry(file.to_string())
        .or_insert_with(|| Held::Over(Hold::default()));
    match entry {
        Held::Over(h) => h.local_refs = Some(count),
        Held::Rehome(r) => r.local_refs = Some(count),
    }
}

/// The `local_ref` faults of `found`: each file with more refs than
/// `baseline` holds it at, naming the first ref past that count.
pub(crate) fn check(found: &Found, baseline: Option<&Baseline>) -> Vec<Value> {
    let mut faults = Vec::new();
    for (file, refs) in found {
        let limit = baseline.map_or(0, |b| held(b, file));
        let Some(first) = refs.get(limit) else {
            continue;
        };
        faults.push(json!({
            "file": file, "fault": "local_ref", "line": first.line, "ref": first.text,
            "size": refs.len(), "limit": limit,
            "fix": format!(
                "`{}` at line {} of `{file}` cites the local tracker, which only this host can open: replace it with what it points at: the person's words with who, date and where; or the command and the number it printed; or delete it",
                first.text, first.line
            ),
        }));
    }
    faults
}

/// `next` (the baseline the budgets ratchet to) holding each file's refs:
/// with no baseline yet (`held` `None`), at its count now; after that, at the
/// smaller of what `held` holds and its count now, dropped at 0, and never
/// added. With the check off (`found` `None`), at what `held` holds.
pub(crate) fn ratchet(found: Option<&Found>, held: Option<&Baseline>, next: &mut Baseline) {
    let Some(found) = found else {
        for (file, held) in held.into_iter().flatten() {
            if count(held) > 0 {
                hold(next, file, count(held));
            }
        }
        return;
    };
    for (file, refs) in found {
        let count = held.map_or(refs.len(), |h| self::held(h, file).min(refs.len()));
        if count > 0 {
            hold(next, file, count);
        }
    }
}

/// Whether `baseline` holds any file's refs: a repo whose baseline holds none
/// has not adopted the check yet.
pub(crate) fn adopted(baseline: &Baseline) -> bool {
    baseline.values().any(|h| count(h) > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> Vec<(usize, String)> {
        find(text, &["rollmind".to_string(), "lore".to_string()])
            .into_iter()
            .map(|r| (r.line, r.text))
            .collect()
    }

    #[test]
    fn a_ref_is_a_listed_name_then_hash_digits() {
        assert_eq!(
            refs("x\n(rollmind#38), lore#7.\n"),
            [(2, "rollmind#38".into()), (2, "lore#7".into())]
        );
        assert_eq!(refs("`rollmind#1`"), [(1, "rollmind#1".into())]);
        assert_eq!(
            refs("https://x.example/a rollmind#2 (rollmind#3"),
            [(1, "rollmind#2".into()), (1, "rollmind#3".into())]
        );
    }

    #[test]
    fn an_owner_a_url_a_longer_name_or_a_word_after_is_no_ref() {
        for text in [
            "sunfmin/lore#25",
            "https://x.example/rollmind#3",
            "https://github.com/sunfmin/rollmind/issues/3",
            "https://x.example/?q=rollmind#3",
            "xrollmind#4 my-lore#5 a.lore#6",
            "rollmind#38x rollmind#3_ rollmind# #12",
        ] {
            assert_eq!(refs(text), [], "{text}");
        }
    }
}
