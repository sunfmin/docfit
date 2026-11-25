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
        if !over(bytes, doc.budget.bytes, held.bytes) && !over(line, doc.budget.line, held.line) {
            continue;
        }
        out.push(Debt {
            kind: TaskKind::OverBudget,
            file: doc.file,
            summary: format!(
                "`{}` is {bytes} bytes, longest line {line}, over its budget of {} bytes, {} characters a line: cut it to what its kind holds",
                doc.file, doc.budget.bytes, doc.budget.line
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
        assert_eq!(debt(&docs, &[], &baseline)[0].kind, TaskKind::OverBudget);
    }
}
