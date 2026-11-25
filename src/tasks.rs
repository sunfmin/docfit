//! Decides `docfit next`: the debt the baseline holds, as tasks. A file held
//! to rehome that is still registered under no kind is a `rehome` task; a
//! document held over its budget that is still over it is an `over-budget`
//! task. Pure: the files measured and the baseline arrive as arguments.

use crate::budget::{Baseline, Held, Measured, Rehome, Size, Stray};
use serde_json::{Value, json};

/// A kind of task, in the order `docfit next` answers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskKind {
    Rehome,
    OverBudget,
}

impl TaskKind {
    pub(crate) const ALL: [TaskKind; 2] = [TaskKind::Rehome, TaskKind::OverBudget];

    pub(crate) fn name(self) -> &'static str {
        match self {
            TaskKind::Rehome => "rehome",
            TaskKind::OverBudget => "over-budget",
        }
    }
}

/// One file of debt: its kind, its path, a line saying what it is, and what
/// was measured of it (without what `docfit where` sees in it).
#[derive(Debug, PartialEq)]
pub(crate) struct Debt<'a> {
    pub(crate) kind: TaskKind,
    pub(crate) file: &'a str,
    pub(crate) summary: String,
    pub(crate) evidence: Value,
}

/// The debt `baseline` holds of `strays` and `docs`: rehome first, then
/// over-budget, each in path order. A file it holds that is neither (gone,
/// registered since, or back within its budget) is none: `docfit ratchet`
/// drops it.
pub(crate) fn debt<'a>(
    docs: &[Measured<'a>],
    strays: &[Stray<'a>],
    baseline: &Baseline,
) -> Vec<Debt<'a>> {
    let mut out = Vec::new();
    for stray in strays {
        if let Some(Held::Rehome(Rehome { rehome })) = baseline.get(stray.file) {
            out.push(Debt {
                kind: TaskKind::Rehome,
                file: stray.file,
                summary: format!(
                    "`{}` ({} bytes) is registered under no kind, held to rehome: move its content to its homes",
                    stray.file, stray.bytes
                ),
                evidence: json!({ "size": stray.bytes, "held": rehome }),
            });
        }
    }
    for doc in docs {
        let Some(Held::Over(held)) = baseline.get(doc.file) else {
            continue;
        };
        let over =
            |size: usize, budget: usize, held: Option<usize>| held.is_some() && size > budget;
        let Size { bytes, line } = doc.size;
        let mut measures = Vec::new();
        if over(bytes, doc.budget.bytes, held.bytes) {
            measures.push(format!("{bytes} bytes, over {}", doc.budget.bytes));
        }
        if over(line, doc.budget.line, held.line) {
            measures.push(format!(
                "its longest line {line} characters, over {}",
                doc.budget.line
            ));
        }
        if measures.is_empty() {
            continue;
        }
        out.push(Debt {
            kind: TaskKind::OverBudget,
            file: doc.file,
            summary: format!(
                "`{}` is {}, its budget: cut it to what its kind holds",
                doc.file,
                measures.join(", and ")
            ),
            evidence: json!({ "size": doc.size, "budget": doc.budget, "held": held }),
        });
    }
    out
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
        assert_eq!(debt(&[], &strays, &Baseline::new()), vec![]);
        let mut baseline = Baseline::new();
        baseline.insert("NEW.md".into(), Held::Rehome(Rehome { rehome: 9 }));
        assert_eq!(debt(&[], &strays, &baseline)[0].kind, TaskKind::Rehome);
    }

    #[test]
    fn a_held_measure_back_within_budget_is_no_debt() {
        let (budget, size) = (Size { bytes: 10, line: 5 }, Size { bytes: 8, line: 9 });
        let docs = [Measured {
            file: "R.md",
            budget,
            size,
            at: 1,
        }];
        let mut baseline = Baseline::new();
        baseline.insert(
            "R.md".into(),
            Held::Over(Hold {
                bytes: Some(12),
                line: None,
            }),
        );
        assert_eq!(debt(&docs, &[], &baseline), vec![]);
        baseline.insert(
            "R.md".into(),
            Held::Over(Hold {
                bytes: None,
                line: Some(9),
            }),
        );
        let line_only = &debt(&docs, &[], &baseline)[0];
        assert_eq!(line_only.kind, TaskKind::OverBudget);
        assert_eq!(
            line_only.summary,
            "`R.md` is its longest line 9 characters, over 5, its budget: cut it to what its kind holds"
        );
    }
}
