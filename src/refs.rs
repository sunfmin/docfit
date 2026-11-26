//! Decides `local_ref`: the refs to the local tracker in a repo's text files,
//! by the rule `docfit help local-refs` states (or, while `docfit.toml` still
//! has [`KEY`], by the names it lists, as docfit 0.1.3 did), against the count
//! the baseline holds each file at, which only shrinks. Pure: the files'
//! texts, the rule and the baseline arrive as arguments.

use crate::budget::{Baseline, Held, Hold};
use crate::tasks::Debt;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The key of `docfit.toml`, from docfit 0.1.3, that lists the names whose
/// `<name>#<digits>` cites the local tracker. While it is there the refs are
/// counted by it; `docfit ratchet --local-refs` deletes it.
pub(crate) const KEY: &str = "local_issue_refs";

/// The key of `docfit.toml` that lists the names whose `<name>#<digits>` is
/// no ref: names that are not trackers in this repo (`closure`).
pub(crate) const NOT: &str = "not_issue_refs";

/// How refs are told: by the names [`KEY`] lists (docfit 0.1.3's), or as
/// every name but those [`NOT`] lists.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Rule {
    Listed(Vec<String>),
    Unlisted(Vec<String>),
}

/// A char a listed name is made of: a ref's name runs back over these.
pub(crate) fn named(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// A char a name the rule reads is made of, after its first letter.
fn unlisted(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_')
}

/// Whether `name` is one the rule reads: `[a-z][a-z0-9_-]*`.
pub(crate) fn is_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase()) && name.chars().all(unlisted)
}

/// One local ref: the line (from 1) it is on, and the ref as written.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct Ref {
    pub(crate) line: usize,
    #[serde(rename = "ref")]
    pub(crate) text: String,
}

/// Where the name of the ref whose `#` is at `i` in `line` starts, by `rule`;
/// `None` when it is no ref's name.
fn name_at(line: &str, i: usize, rule: &Rule) -> Option<usize> {
    match rule {
        Rule::Listed(names) => {
            let start = line[..i].trim_end_matches(named).len();
            let listed = names.iter().any(|name| *name == line[start..i]);
            (listed && !line[..start].ends_with('/')).then_some(start)
        }
        Rule::Unlisted(not) => {
            let start = line[..i].trim_end_matches(unlisted).len();
            let name = &line[start..i];
            let before = line[..start].chars().next_back();
            let run_on = before.is_some_and(|c| c.is_alphanumeric() || "/.:{".contains(c));
            (is_name(name) && !run_on && !not.iter().any(|n| n == name)).then_some(start)
        }
    }
}

/// The local refs in `text`, in order: `<name>#<digits>`, not run on by a
/// letter, digit or `_` after it, and not in a URL (a word with `://`).
/// By the rule (`Unlisted`), `<name>` is `[a-z][a-z0-9_-]*`, not one it
/// lists, and not after a letter, a digit, `/` (an owner's,
/// `sunfmin/lore#25`), `.` (`a.md#3`), `:` or `{` (`{closure#0}`). By a list
/// (`Listed`), `<name>` is one it lists, not run on by a name's char before
/// it, and not after a `/`.
pub(crate) fn find(text: &str, rule: &Rule) -> Vec<Ref> {
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
            let Some(start) = name_at(line, i, rule) else {
                continue;
            };
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
                "`{}` at line {} of `{file}` cites the local tracker, which only this host can open: replace it with what it points at: the person's words with who, date and where; or the command and the number it printed; or delete it. Not a tracker? `docfit help local-refs`",
                first.text, first.line
            ),
        }));
    }
    faults
}

/// `next` (the baseline the budgets ratchet to) holding each file's refs:
/// with no baseline yet (`held` `None`), at its count now; after that, at the
/// smaller of what `held` holds and its count now, dropped at 0, and never
/// added.
pub(crate) fn ratchet(found: &Found, held: Option<&Baseline>, next: &mut Baseline) {
    for (file, refs) in found {
        let count = held.map_or(refs.len(), |h| self::held(h, file).min(refs.len()));
        if count > 0 {
            hold(next, file, count);
        }
    }
}

/// The `local-refs` debt: each file of `found` whose refs `baseline` holds,
/// with every ref and its line.
pub(crate) fn debt<'a>(found: &'a Found, baseline: &Baseline) -> Vec<Debt<'a>> {
    let held = |file: &str| Some(held(baseline, file)).filter(|h| *h > 0);
    let debt = |(file, refs): (&'a String, &'a Vec<Ref>)| {
        let held = held(file)?;
        Some(Debt {
            file,
            summary: format!(
                "`{file}` cites the local tracker {} times, held at {held}: replace each ref with what it points at, or delete it",
                refs.len()
            ),
            evidence: json!({ "refs": refs, "held": held }),
        })
    };
    found.iter().filter_map(debt).collect()
}

/// Whether `baseline` holds any file's refs: a repo whose baseline holds none
/// has not adopted the check yet.
pub(crate) fn adopted(baseline: &Baseline) -> bool {
    baseline.values().any(|h| count(h) > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, rule: &Rule) -> Vec<(usize, String)> {
        find(text, rule)
            .into_iter()
            .map(|r| (r.line, r.text))
            .collect()
    }

    fn listed(text: &str) -> Vec<(usize, String)> {
        let names = vec!["rollmind".to_string(), "lore".to_string()];
        found(text, &Rule::Listed(names))
    }

    fn refs(text: &str) -> Vec<(usize, String)> {
        found(text, &Rule::Unlisted(vec!["closure".to_string()]))
    }

    #[test]
    fn a_listed_ref_is_a_listed_name_then_hash_digits() {
        assert_eq!(
            listed("x\n(rollmind#38), lore#7. guild#66\n"),
            [(2, "rollmind#38".into()), (2, "lore#7".into())]
        );
        assert_eq!(listed("`rollmind#1`"), [(1, "rollmind#1".into())]);
        assert_eq!(
            listed("https://x.example/a rollmind#2 (rollmind#3"),
            [(1, "rollmind#2".into()), (1, "rollmind#3".into())]
        );
    }

    #[test]
    fn an_owner_a_url_a_longer_name_or_a_word_after_is_no_listed_ref() {
        for text in [
            "sunfmin/lore#25",
            "https://x.example/rollmind#3",
            "https://github.com/sunfmin/rollmind/issues/3",
            "https://x.example/?q=rollmind#3",
            "xrollmind#4 my-lore#5 a.lore#6",
            "rollmind#38x rollmind#3_ rollmind# #12",
        ] {
            assert_eq!(listed(text), [], "{text}");
        }
    }

    #[test]
    fn by_the_rule_any_lowercase_name_then_hash_digits_is_a_ref() {
        assert_eq!(
            refs("x\n// (guild#66), my-tool_2#7. `kiln#1`\n"),
            [
                (2, "guild#66".into()),
                (2, "my-tool_2#7".into()),
                (2, "kiln#1".into())
            ]
        );
        assert_eq!(refs("https://x.example/a kiln#2"), [(1, "kiln#2".into())]);
    }

    #[test]
    fn by_the_rule_an_owner_a_url_a_path_a_symbol_or_a_named_non_tracker_is_no_ref() {
        for text in [
            "sunfmin/kiln#25",
            "https://x/issues/3 https://x.example/kiln#3 https://x.example/?q=kiln#3",
            "{closure#0} {shim#1} a.md#3 C#1 Lore#4 x:kiln#5",
            "closure#2",
            "kiln#38x kiln#3_ kiln# #12 _x#1 -x#1 9x#1",
            "日本kiln#1",
        ] {
            assert_eq!(refs(text), [], "{text}");
        }
    }
}
