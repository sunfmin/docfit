//! Decides `docfit next`: the debt the baseline holds, as tasks, each with
//! its kind, the help topic whose steps pay it, and its accept. A file held
//! to rehome that is still registered under no kind is a `rehome` task; a
//! document held over its budget that is still over it is an `over-budget`
//! task. A file held at a count of local refs, restatements or measurements
//! that still holds one is a task too, its debt decided where its count is;
//! a hot document is one, which no baseline holds; and a `docfit.toml` that
//! still lists `local_issue_refs` is one. Pure: the files judged, the
//! baseline and the history arrive as arguments.

use crate::budget::{BASELINE, Baseline, Held, Measured, Rehome, Size, Stray};
use crate::cli::EXIT_OK;
use crate::documents::CONFIG;
use crate::hot::{self, Commit, Hot};
use crate::survey::Judged;
use crate::{measurements, refs, restates};
use serde_json::{Map, Value, json};

/// The topic whose steps pay a `rehome` or an `over-budget` task.
pub(crate) const REHOME: &str = "rehome";

/// The kind of `docfit next`'s task to drop `local_issue_refs`.
pub(crate) const DROP: &str = "drop-local-issue-refs";

/// One task of `docfit next`: its kind, the topic of `docfit help` whose
/// steps pay it, its debt, and when it is done.
pub(crate) struct Task<'a> {
    pub(crate) kind: &'static str,
    pub(crate) topic: &'static str,
    pub(crate) debt: Debt<'a>,
    pub(crate) accept: String,
}

/// Every task of `judged`'s debt, in the order `docfit next` answers them,
/// and each kind's count of them.
pub(crate) fn list<'a>(
    judged: &Judged<'a>,
    held: &Baseline,
    rule: &'a refs::Rule,
    history: &[Commit],
    hot: Hot,
) -> (Vec<Task<'a>>, Map<String, Value>) {
    let (docs, counted) = (&judged.docs, &judged.counted);
    let [rehome, over_budget] = debt(docs, &judged.strays, held);
    // Each kind: its name, the topic of its steps, what the baseline holds
    // its file at until it is done (none: the hot file, which no baseline
    // holds, is done within its budget), and its debt.
    let debts = [
        ("rehome", REHOME, Some("to rehome"), rehome),
        ("over-budget", REHOME, Some("over its budget"), over_budget),
        (
            "local-refs",
            "local-refs",
            Some("at a count of local refs"),
            refs::debt(counted.refs, held),
        ),
        (
            "restates",
            "restates",
            Some("at a count of restatements"),
            restates::debt(&counted.restated, held),
        ),
        (
            "measurement",
            "measurement",
            Some("at a count of measurements"),
            measurements::debt(&counted.measured, held),
        ),
        ("hot-file", "hot-file", None, hot::debt(docs, history, hot)),
    ];
    let (mut tasks, mut counts) = (Vec::new(), Map::new());
    let listed = match rule {
        refs::Rule::Listed(names) => Some(names),
        refs::Rule::Unlisted(_) => None,
    };
    counts.insert(DROP.to_string(), json!(usize::from(listed.is_some())));
    if let Some(names) = listed {
        tasks.push(Task {
            kind: DROP,
            topic: DROP,
            debt: Debt {
                file: CONFIG,
                summary: format!("{CONFIG} still lists `{}`, docfit 0.1.3's: a ref to a name it does not list, such as a checkout added or gone since, goes uncounted; drop it for the rule, which needs no list", refs::KEY),
                evidence: json!({ refs::KEY: names }),
            },
            accept: format!("{CONFIG} has no `{}`, and `docfit check` exits {EXIT_OK}", refs::KEY),
        });
    }
    for (kind, topic, holds, debts) in debts {
        counts.insert(kind.to_string(), json!(debts.len()));
        for debt in debts {
            let accept = match holds {
                Some(holds) => format!(
                    "`docfit ratchet`, then `docfit check` exits {EXIT_OK} and {BASELINE} no longer holds `{}` {holds}",
                    debt.file
                ),
                None => format!(
                    "`{}` is within its kind's budget (`evidence.budget`), and `docfit check` exits {EXIT_OK}",
                    debt.file
                ),
            };
            tasks.push(Task {
                kind,
                topic,
                debt,
                accept,
            });
        }
    }
    (tasks, counts)
}

/// One file of debt: its path, a line saying what it is, and what was
/// measured of it (without what `docfit where` sees in it).
#[derive(Debug, PartialEq)]
pub(crate) struct Debt<'a> {
    pub(crate) file: &'a str,
    pub(crate) summary: String,
    pub(crate) evidence: Value,
}

/// The debt `baseline` holds of `strays` and `docs`, by kind, in the order
/// `docfit next` answers them: `rehome`, then `over-budget`, each in path
/// order. A file it holds that is neither (gone, registered since, or back
/// within its budget) is none: `docfit ratchet` drops it.
pub(crate) fn debt<'a>(
    docs: &[Measured<'a>],
    strays: &[Stray<'a>],
    baseline: &Baseline,
) -> [Vec<Debt<'a>>; 2] {
    let mut rehome = Vec::new();
    for stray in strays {
        if let Some(Held::Rehome(Rehome { rehome: held, .. })) = baseline.get(stray.file) {
            rehome.push(Debt {
                file: stray.file,
                summary: format!(
                    "`{}` ({} bytes) is registered under no kind, held to rehome: move its content to its homes",
                    stray.file, stray.bytes
                ),
                evidence: json!({ "size": stray.bytes, "held": held }),
            });
        }
    }
    let mut over_budget = Vec::new();
    for doc in docs {
        let Some(Held::Over(held)) = baseline.get(doc.file) else {
            continue;
        };
        let over =
            |size: usize, budget: usize, held: Option<usize>| held.is_some() && size > budget;
        let Size { bytes, paragraph } = doc.size;
        let mut measures = Vec::new();
        if over(bytes, doc.budget.bytes, held.bytes) {
            measures.push(format!("{bytes} bytes, over {}", doc.budget.bytes));
        }
        if over(paragraph, doc.budget.paragraph, held.paragraph) {
            measures.push(format!(
                "its longest paragraph {paragraph} characters, over {}",
                doc.budget.paragraph
            ));
        }
        if measures.is_empty() {
            continue;
        }
        over_budget.push(Debt {
            file: doc.file,
            summary: format!(
                "`{}` is {}, its budget: cut it to what its kind holds",
                doc.file,
                measures.join(", and ")
            ),
            evidence: json!({ "size": doc.size, "budget": doc.budget, "held": held }),
        });
    }
    [rehome, over_budget]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Hold;

    #[test]
    fn a_stray_the_baseline_does_not_hold_to_rehome_is_no_task() {
        let fault = json!({});
        let strays = [Stray {
            file: "NEW.md",
            bytes: 9,
            fault: &fault,
        }];
        assert_eq!(debt(&[], &strays, &Baseline::new())[0], vec![]);
        let mut baseline = Baseline::new();
        baseline.insert(
            "NEW.md".into(),
            Held::Rehome(Rehome {
                rehome: 9,
                local_refs: None,
            }),
        );
        assert_eq!(debt(&[], &strays, &baseline)[0][0].file, "NEW.md");
    }

    #[test]
    fn a_held_measure_at_or_within_budget_is_no_debt() {
        let (budget, size) = (
            Size {
                bytes: 8,
                paragraph: 5,
            },
            Size {
                bytes: 8,
                paragraph: 9,
            },
        );
        let docs = [Measured {
            file: "R.md",
            budget,
            size,
            at: 1,
            broken: Vec::new(),
        }];
        let mut baseline = Baseline::new();
        baseline.insert(
            "R.md".into(),
            Held::Over(Hold {
                bytes: Some(12),
                ..Hold::default()
            }),
        );
        assert_eq!(debt(&docs, &[], &baseline)[1], vec![]);
        baseline.insert(
            "R.md".into(),
            Held::Over(Hold {
                paragraph: Some(9),
                ..Hold::default()
            }),
        );
        let [_, over] = debt(&docs, &[], &baseline);
        assert_eq!(
            over[0].summary,
            "`R.md` is its longest paragraph 9 characters, over 5, its budget: cut it to what its kind holds"
        );
    }
}
