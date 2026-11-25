//! Decides `docfit next`: the debt the baseline holds, as tasks. A file held
//! to rehome that is still registered under no kind is a `rehome` task; a
//! document held over its budget that is still over it is an `over-budget`
//! task. Pure: the files measured and the baseline arrive as arguments.

use crate::budget::{Baseline, Held, Measured, Rehome, Size, Stray};
use serde_json::{Value, json};

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
) -> [(&'static str, Vec<Debt<'a>>); 2] {
    let mut rehome = Vec::new();
    for stray in strays {
        if let Some(Held::Rehome(Rehome { rehome: held })) = baseline.get(stray.file) {
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
    [("rehome", rehome), ("over-budget", over_budget)]
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
        assert_eq!(debt(&[], &strays, &Baseline::new())[0].1, vec![]);
        let mut baseline = Baseline::new();
        baseline.insert("NEW.md".into(), Held::Rehome(Rehome { rehome: 9 }));
        assert_eq!(debt(&[], &strays, &baseline)[0].1[0].file, "NEW.md");
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
        assert_eq!(debt(&docs, &[], &baseline)[1].1, vec![]);
        baseline.insert(
            "R.md".into(),
            Held::Over(Hold {
                paragraph: Some(9),
                ..Hold::default()
            }),
        );
        let [_, (kind, over)] = debt(&docs, &[], &baseline);
        assert_eq!(kind, "over-budget");
        assert_eq!(
            over[0].summary,
            "`R.md` is its longest paragraph 9 characters, over 5, its budget: cut it to what its kind holds"
        );
    }
}
