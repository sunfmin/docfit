//! Decides `docfit next`'s `hot-file` tasks: a registered document changed in
//! more than a share of the last commits of HEAD's first-parent history.
//! Being hot is a signal, not a fault, so `docfit check` never reads it. Pure:
//! the history and the documents measured arrive as arguments.

use crate::budget::Measured;
use crate::documents::{CONFIG, Refusal};
use crate::tasks::Debt;
use serde_json::json;

/// The key of `docfit.toml` that sets [`Hot`].
pub(crate) const KEY: &str = "hot";
/// How many of a hot file's last changes a task shows.
pub(crate) const SHOWN: usize = 10;

/// When a document is hot: changed in more than `share` of the last `window`
/// first-parent commits.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct Hot {
    pub(crate) window: usize,
    pub(crate) share: f64,
}

impl Default for Hot {
    fn default() -> Hot {
        Hot {
            window: 100,
            share: 0.25,
        }
    }
}

/// The default as `docfit.toml` writes it, for the manual.
pub(crate) fn shown() -> String {
    let Hot { window, share } = Hot::default();
    format!("{KEY} = {{ window = {window}, share = {share} }}")
}

/// `hot = { window = <n>, share = <x> }` read; [`Hot::default`] without it.
pub(crate) fn read(value: Option<&toml::Value>) -> Result<Hot, Refusal> {
    let Some(value) = value else {
        return Ok(Hot::default());
    };
    let hot = value.clone().try_into::<Hot>().ok();
    hot.filter(|h| h.window > 0 && h.share > 0.0 && h.share < 1.0)
        .ok_or_else(|| Refusal {
            fault: format!("`{KEY}` = `{value}` is not when a document is hot"),
            fix: format!(
                "write `{}` in {CONFIG}, before its first table: `window` the last first-parent commits read, at least 1, and `share` of them a document changes in to be hot, between 0 and 1",
                shown()
            ),
        })
}

/// One first-parent commit: its short hash, its subject, and the files it
/// changed against its first parent.
#[derive(Debug, PartialEq)]
pub(crate) struct Commit {
    pub(crate) commit: String,
    pub(crate) subject: String,
    pub(crate) files: Vec<String>,
}

/// Each of `docs` changed in more than `hot.share` of `hot.window` commits of
/// `history` (newest first, at most `hot.window` of them), in path order. The
/// share is of the window, so a young repo's few commits make nothing hot.
pub(crate) fn debt<'a>(docs: &[Measured<'a>], history: &[Commit], hot: Hot) -> Vec<Debt<'a>> {
    let history = &history[..history.len().min(hot.window)];
    let mut out = Vec::new();
    for doc in docs {
        let changes: Vec<&Commit> = history
            .iter()
            .filter(|c| c.files.iter().any(|f| f == doc.file))
            .collect();
        if changes.len() as f64 <= hot.share * hot.window as f64 {
            continue;
        }
        let shown: Vec<_> = changes
            .iter()
            .take(SHOWN)
            .map(|c| json!({ "commit": c.commit, "subject": c.subject }))
            .collect();
        out.push(Debt {
            file: doc.file,
            summary: format!(
                "`{}` changed in {} of the last {} first-parent commits, more than {}: move what the changes added to its homes, so the next change need not touch it",
                doc.file,
                changes.len(),
                hot.window,
                hot.share
            ),
            evidence: json!({
                "share": changes.len() as f64 / hot.window as f64,
                "changed": changes.len(),
                "window": hot.window,
                "commits": shown,
                "size": doc.size,
                "budget": doc.budget,
            }),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Size;

    fn doc(file: &str) -> Measured<'_> {
        let size = Size {
            bytes: 1,
            paragraph: 1,
        };
        Measured {
            file,
            budget: size,
            size,
            at: 1,
            broken: Vec::new(),
        }
    }

    /// `n` commits, newest first, the `i`th changing `A.md` when `a(i)`.
    fn history(n: usize, a: impl Fn(usize) -> bool) -> Vec<Commit> {
        (0..n)
            .map(|i| Commit {
                commit: format!("c{i}"),
                subject: format!("change {i}"),
                files: if a(i) { vec!["A.md".into()] } else { vec![] },
            })
            .collect()
    }

    #[test]
    fn a_document_is_hot_past_the_share_of_the_window_not_at_it() {
        let docs = [doc("A.md")];
        let at = history(100, |i| i < 25);
        assert_eq!(debt(&docs, &at, Hot::default()), vec![]);
        let past = history(100, |i| i < 26);
        let hot = debt(&docs, &past, Hot::default());
        assert_eq!(hot[0].evidence["changed"], 26);
        assert_eq!(hot[0].evidence["share"], 0.26);
        let commits = hot[0].evidence["commits"].as_array().unwrap();
        assert_eq!(commits.len(), SHOWN);
        assert_eq!(commits[0], json!({ "commit": "c0", "subject": "change 0" }));
    }

    #[test]
    fn the_share_is_of_the_window_and_only_the_window_is_read() {
        let docs = [doc("A.md")];
        // 4 of 4 commits: a young repo, nothing hot.
        assert_eq!(debt(&docs, &history(4, |_| true), Hot::default()), vec![]);
        // Every change past the window is not read.
        let old = history(200, |i| i >= 100);
        assert_eq!(debt(&docs, &old, Hot::default()), vec![]);
        let small = Hot {
            window: 4,
            share: 0.5,
        };
        assert_eq!(debt(&docs, &history(4, |i| i < 3), small).len(), 1);
    }

    #[test]
    fn hot_reads_either_number_and_refuses_one_out_of_range() {
        let value = |s: &str| s.parse::<toml::Table>().unwrap()["hot"].clone();
        assert_eq!(read(None).ok(), Some(Hot::default()));
        let share = read(Some(&value("hot = { share = 0.1 }"))).ok().unwrap();
        assert_eq!((share.window, share.share), (100, 0.1));
        for bad in [
            "hot = { share = 1.5 }",
            "hot = { window = 0 }",
            "hot = { windows = 9 }",
            "hot = 3",
        ] {
            let refused = read(Some(&value(bad))).unwrap_err();
            assert!(refused.fix.contains("window = 100"), "{bad}");
        }
    }
}
