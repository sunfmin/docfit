//! Decides `restates`: what a document copies of what the repo already says,
//! a quoted string a string catalog holds verbatim and a run of [`RUN`] or
//! more non-blank lines another tracked file holds, against the count the
//! baseline holds each document at, which only shrinks. `docfit where` lists
//! what [`find`] finds, and `docfit check` counts it: one detector. Pure: the
//! texts, the catalogs and the baseline arrive as arguments.

use crate::budget::{self, Baseline, Slot};
use crate::paragraph::{self, Paragraph};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

/// The fewest non-blank lines a copied run has.
pub(crate) const RUN: usize = 3;

/// A tracked text file: its path from the repo's top, and its text.
pub(crate) struct Text {
    pub(crate) path: String,
    pub(crate) text: String,
}

/// One restatement: the line (from 1) it starts on, the last line it is on,
/// what it is (the string, or the run's first line), and where the repo
/// already says it (the catalog, or `<file>:<line>`).
#[derive(Debug, PartialEq)]
pub(crate) struct Restated {
    pub(crate) line: usize,
    pub(crate) end: usize,
    pub(crate) what: String,
    pub(crate) from: String,
}

/// What a document restates, each in line order.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Restates {
    pub(crate) strings: Vec<Restated>,
    pub(crate) copies: Vec<Restated>,
}

/// A line, trimmed, with the line (from 1) it is on.
type Line<'a> = (usize, &'a str);

/// Every run of [`RUN`] non-blank lines of the files it is made from, each
/// with where it is: the file's path, and the line it starts on.
pub(crate) struct Index<'a>(HashMap<Vec<&'a str>, Vec<(&'a str, usize)>>);

/// `text`'s non-blank lines, trimmed.
fn lines(text: &str) -> Vec<Line<'_>> {
    text.lines()
        .enumerate()
        .map(|(n, l)| (n + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect()
}

impl<'a> Index<'a> {
    pub(crate) fn new(texts: &'a [Text]) -> Index<'a> {
        let mut at: HashMap<Vec<&str>, Vec<(&str, usize)>> = HashMap::new();
        for t in texts {
            for run in lines(&t.text).windows(RUN) {
                let key = run.iter().map(|l| l.1).collect();
                at.entry(key).or_default().push((&t.path, run[0].0));
            }
        }
        Index(at)
    }
}

/// What `file`'s `text`, cut into `paragraphs`, restates: each quoted string
/// outside fenced code and code spans that a catalog of `catalogs` holds
/// verbatim as a quoted string, at the line its paragraph starts on; and each
/// run of [`RUN`] or more non-blank lines, trimmed, that a file of `index`
/// other than `file` holds in a row, blank lines skipped, unless the run is
/// in a fenced block that names that file (by its path or its name) on its
/// opening line or the non-blank line before it.
pub(crate) fn find(
    file: &str,
    text: &str,
    paragraphs: &[Paragraph],
    catalogs: &[Text],
    index: &Index,
) -> Restates {
    let strings = paragraph::quotes(paragraphs)
        .into_iter()
        .filter_map(|(line, s)| {
            let literal = serde_json::to_string(&s).expect("a string");
            let from = catalogs.iter().find(|c| c.text.contains(&literal))?;
            Some(Restated {
                line,
                end: line,
                what: s,
                from: from.path.clone(),
            })
        })
        .collect();
    Restates {
        strings,
        copies: copies(file, text, index),
    }
}

/// The runs of `text` copied from a file of `index` other than `file`: each
/// window of [`RUN`] lines found, joined to the run it overlaps.
fn copies(file: &str, text: &str, index: &Index) -> Vec<Restated> {
    let names = fences(text);
    let names_source = |run: &[Line], source: &str| {
        let name = source.rsplit('/').next().unwrap_or(source);
        let fence = names.get(&run[0].0);
        fence.is_some_and(|(block, named)| {
            run.iter()
                .all(|l| names.get(&l.0).is_some_and(|f| f.0 == *block))
                && (whole(named, source) || whole(named, name))
        })
    };
    let mut out: Vec<Restated> = Vec::new();
    // The first window after the last run's end.
    let mut after = 0;
    for (i, run) in lines(text).windows(RUN).enumerate() {
        let key: Vec<&str> = run.iter().map(|l| l.1).collect();
        let found = index.0.get(&key).and_then(|at| {
            at.iter()
                .find(|&&(path, _)| path != file && !names_source(run, path))
        });
        let Some((path, line)) = found else {
            continue;
        };
        let end = run[RUN - 1].0;
        match out.last_mut() {
            Some(last) if i < after => last.end = end,
            _ => out.push(Restated {
                line: run[0].0,
                end,
                what: run[0].1.to_string(),
                from: format!("{path}:{line}"),
            }),
        }
        after = i + RUN;
    }
    out
}

/// Whether `text` holds `name` not run on by a char of a path's name.
fn whole(text: &str, name: &str) -> bool {
    let named = |c: char| c.is_alphanumeric() || "_-.".contains(c);
    text.match_indices(name).any(|(i, _)| {
        !text[..i].ends_with(named)
            && !text[i + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    })
}

/// Each line of `text` in a fenced block, its fences' lines among them: the
/// block's number, and what names its source, its opening line and the
/// non-blank line before it.
fn fences(text: &str) -> HashMap<usize, (usize, String)> {
    let mut out = HashMap::new();
    let (mut open, mut before, mut blocks): (Option<String>, &str, usize) = (None, "", 0);
    for (n, line) in text.lines().enumerate() {
        let t = line.trim();
        let fence = t.starts_with("```") || t.starts_with("~~~");
        match (&open, fence) {
            (None, true) => {
                blocks += 1;
                open = Some(format!("{before}\n{t}"));
            }
            (Some(_), true) => {
                out.insert(n + 1, (blocks, open.take().expect("open")));
                continue;
            }
            _ => {}
        }
        if let Some(named) = &open {
            out.insert(n + 1, (blocks, named.clone()));
        } else if !t.is_empty() {
            before = t;
        }
    }
    out
}

/// What a document restates, as the baseline holds and a fault names it.
#[derive(Clone, Copy)]
enum Measure {
    Strings,
    Copies,
}

impl Measure {
    const ALL: [Measure; 2] = [Measure::Strings, Measure::Copies];

    fn name(self) -> &'static str {
        match self {
            Measure::Strings => "strings",
            Measure::Copies => "copies",
        }
    }

    /// What `first`, the first restatement of `file` past what is held,
    /// is, and how it is fixed.
    fn fix(self, file: &str, first: &Restated) -> String {
        let (line, end, from) = (first.line, first.end, &first.from);
        match self {
            Measure::Strings => format!(
                "line {line} of `{file}` quotes \"{}\", which `{from}` holds: point at the key, or at the screen test that shows it, instead",
                first.what
            ),
            Measure::Copies => format!(
                "lines {line} to {end} of `{file}` copy `{from}`: keep it in one place and point at it"
            ),
        }
    }

    fn of(self, restates: &Restates) -> &[Restated] {
        match self {
            Measure::Strings => &restates.strings,
            Measure::Copies => &restates.copies,
        }
    }

    /// The field of a hold that holds it.
    fn slot(self) -> Slot {
        match self {
            Measure::Strings => |h| &mut h.strings,
            Measure::Copies => |h| &mut h.copies,
        }
    }

    /// Each document of `found`, with its count now.
    fn counts<'a>(
        self,
        found: &'a BTreeMap<&str, Restates>,
    ) -> impl Iterator<Item = (&'a str, usize)> {
        found.iter().map(move |(f, r)| (*f, self.of(r).len()))
    }
}

/// The `restates` faults of `found`: for each document and measure, when it
/// has more than `baseline` holds it at, one naming the first past that.
pub(crate) fn check(found: &BTreeMap<&str, Restates>, baseline: Option<&Baseline>) -> Vec<Value> {
    let mut faults = Vec::new();
    for (file, restates) in found {
        for measure in Measure::ALL {
            let all = measure.of(restates);
            let limit = baseline.map_or(0, |b| budget::held(b, file, measure.slot()));
            let Some(first) = all.get(limit) else {
                continue;
            };
            faults.push(json!({
                "file": file, "fault": "restates", "restates": measure.name(), "line": first.line,
                "found": first.what, "from": first.from, "size": all.len(), "limit": limit,
                "fix": measure.fix(file, first),
            }));
        }
    }
    faults
}

/// `next` (the baseline the budgets ratchet to) holding each document's
/// counts, shrink-only ([`budget::ratchet_counts`]).
pub(crate) fn ratchet(
    found: &BTreeMap<&str, Restates>,
    held: Option<&Baseline>,
    next: &mut Baseline,
) {
    for measure in Measure::ALL {
        budget::ratchet_counts(measure.counts(found), measure.slot(), held, next);
    }
}

/// `next` holding each document's count now, of each measure it holds no
/// document's count of yet: `docfit init`'s adoption, which leaves a measure
/// already adopted as it is.
pub(crate) fn adopt(found: &BTreeMap<&str, Restates>, next: &mut Baseline) {
    for measure in Measure::ALL {
        budget::adopt_counts(measure.counts(found), measure.slot(), next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copies_of(doc: &str, source: &str) -> Vec<(usize, usize)> {
        let texts = [Text {
            path: "src/a.rs".into(),
            text: source.into(),
        }];
        copies("README.md", doc, &Index::new(&texts))
            .iter()
            .map(|c| (c.line, c.end))
            .collect()
    }

    #[test]
    fn windows_that_overlap_join_one_run() {
        assert_eq!(copies_of("a\nb\nc\nd\n", "a\nb\nc\nd\n"), [(1, 4)]);
        // `b c d` is not in the source, `c d e` is: one run, lines 1 to 5.
        assert_eq!(
            copies_of("a\nb\nc\nd\ne\n", "a\nb\nc\n\nz\nc\nd\ne\n"),
            [(1, 5)]
        );
        assert_eq!(
            copies_of("a\nb\nc\nx\ny\nz\nq\n", "a\nb\nc\n\nx\ny\nz\n"),
            [(1, 6)]
        );
        assert_eq!(
            copies_of("a\nb\nc\n- \na\nb\nc\n", "a\nb\nc\n"),
            [(1, 3), (5, 7)]
        );
    }

    #[test]
    fn a_fence_names_its_source_only_inside_it_and_by_a_whole_name() {
        let doc = "From `src/a.rs`:\n```\na\nb\nc\n```\n\na\nb\nc\n";
        assert_eq!(copies_of(doc, "a\nb\nc\n"), [(8, 10)]);
        for named in ["`xa.rs`", "`a.rsx`", "`src/a.rs`", "`a.rs`"] {
            let doc = format!("From {named}:\n```\na\nb\nc\n```\n");
            let exempt = !named.contains('x');
            assert_eq!(copies_of(&doc, "a\nb\nc\n").is_empty(), exempt, "{named}");
        }
    }
}
