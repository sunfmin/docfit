//! What docfit reads of a repo, and how what it read is judged against the
//! baseline. A command asks for it in two steps, each reading the repo once:
//!
//! - [`Survey`]: a `docfit.toml` read against the repo, the documents it
//!   registers, the paths git tracks, and the text files and catalogs the
//!   checks count in. `docfit where` needs no more.
//! - [`Judged`]: each document's text read, measured, its broken pointers
//!   found, each stray sized, and the counts taken; then its `faults`
//!   against a baseline, the baseline it `ratcheted` to, and the one `init`
//!   `adopted`.
//!
//! A new check is wired here, in [`Counted`] and [`Judged`]: the commands
//! only ask for its faults, its ratchet and its debt.

use crate::budget::{self, BASELINE, Baseline, Measured, Size, Stray};
use crate::cli::refusal;
use crate::documents::{self, Answer, CATALOGS, CONFIG, Config, Document, Refusal, TABLE};
use crate::kind::Kind;
use crate::measurements;
use crate::paragraph::{self, Adr, Home, Known};
use crate::pointers::{self, Tree};
use crate::refs::{self, Found};
use crate::repo;
use crate::restates::{self, Index, Restates, Text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The repo's `docfit.toml`: `None` when it has none.
pub(crate) fn config(top: &Path) -> Result<Option<String>, Vec<Refusal>> {
    match std::fs::read_to_string(top.join(CONFIG)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(vec![refusal(
            &format!("cannot read {CONFIG} at the repo's top: {e}"),
            &format!("make {CONFIG} readable UTF-8 text, or restore it from git"),
        )]),
    }
}

/// The repo's `docfit.toml`, refused when it has none.
pub(crate) fn written(top: &Path) -> Result<String, Vec<Refusal>> {
    config(top)?.ok_or_else(|| {
        vec![refusal(
            &format!("there is no {CONFIG} at the repo's top"),
            &format!("run `docfit init`, or write {CONFIG} with a `[{TABLE}]` table, a line `\"<glob>\" = \"<kind>\"` per document (see `docfit check --help`)"),
        )]
    })
}

/// The baseline at the repo's top: `None` when it has none yet.
pub(crate) fn baseline(top: &Path) -> Result<Option<Baseline>, Vec<Refusal>> {
    match std::fs::read_to_string(top.join(BASELINE)) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|e| {
            vec![refusal(
                &format!("{BASELINE} is not a baseline: {e}"),
                &format!("restore {BASELINE} from git, or delete it and run `docfit ratchet`"),
            )]
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(vec![refusal(
            &format!("cannot read {BASELINE}: {e}"),
            &format!("make {BASELINE} readable, or restore it from git"),
        )]),
    }
}

/// A `docfit.toml` read against the repo at `top`: the documents it
/// registers, the paths git tracks, and the text files `docfit.toml` does not
/// ignore, the local refs in them, and its string catalogs.
pub(crate) struct Survey {
    pub(crate) answer: Answer,
    pub(crate) tracked: Vec<String>,
    pub(crate) config: Config,
    kept: Vec<Text>,
    found: Found,
    catalogs: Vec<Text>,
}

impl Survey {
    /// The repo's own `docfit.toml`, refused when it has none.
    pub(crate) fn at(top: &Path) -> Result<Survey, Vec<Refusal>> {
        Survey::of(top, &written(top)?, repo::tracked(top)?)
    }

    /// `text`, a `docfit.toml`, read against the repo at `top`, whose paths
    /// git tracks are `tracked`.
    pub(crate) fn of(top: &Path, text: &str, tracked: Vec<String>) -> Result<Survey, Vec<Refusal>> {
        let config = documents::config(text)?;
        let answer = documents::check(&config, &tracked);
        let kept = texts(top, config.kept(&tracked));
        Ok(Survey {
            found: found(&config, &kept),
            catalogs: catalogs(top, &config, &tracked)?,
            kept,
            answer,
            tracked,
            config,
        })
    }

    /// Every registered document read, measured and counted, and every file
    /// registered under no kind sized: git is asked once, for every pointer
    /// of them all, which it ignores.
    pub(crate) fn judged(&self, top: &Path) -> Result<Judged<'_>, Vec<Refusal>> {
        let texts = read(top, &self.answer)?;
        let docs = measured(top, &texts, &self.tracked)?;
        Ok(Judged {
            answer: &self.answer,
            strays: strays(top, &self.answer)?,
            counted: self.counted(),
            texts,
            docs,
        })
    }

    /// What is counted in the files: the local refs, and what each registered
    /// document restates, against the other text files and the catalogs, and
    /// measures.
    fn counted(&self) -> Counted<'_> {
        let index = Index::new(&self.kept);
        let (mut restated, mut measured) = (BTreeMap::new(), measurements::Found::new());
        for t in &self.kept {
            let Some(doc) = self.answer.documents.get(&t.path) else {
                continue;
            };
            let paragraphs = paragraph::split(&t.text);
            let found = restates::find(&t.path, &t.text, &paragraphs, &self.catalogs, &index);
            restated.insert(t.path.as_str(), found);
            if let Some(found) = measurements::find(doc.kind, &paragraphs) {
                measured.insert(t.path.as_str(), found);
            }
        }
        Counted {
            refs: &self.found,
            restated,
            measured,
        }
    }
}

/// What the baseline holds a count of in each file: its local refs, and a
/// document's restatements and measurements.
pub(crate) struct Counted<'a> {
    pub(crate) refs: &'a Found,
    pub(crate) restated: BTreeMap<&'a str, Restates>,
    pub(crate) measured: measurements::Found<'a>,
}

/// A survey's documents read and judged: each document's text, measured,
/// each stray, and the counts.
pub(crate) struct Judged<'a> {
    answer: &'a Answer,
    texts: Vec<(&'a str, Document, String)>,
    pub(crate) docs: Vec<Measured<'a>>,
    pub(crate) strays: Vec<Stray<'a>>,
    pub(crate) counted: Counted<'a>,
}

impl Judged<'_> {
    /// The bytes an agent must read before any work: the agent documents'
    /// total, as read now.
    pub(crate) fn must_read(&self) -> usize {
        self.answer.must_read(|f| {
            self.texts
                .iter()
                .find(|(file, ..)| *file == f)
                .map_or(0, |(.., text)| text.len())
        })
    }

    /// Every fault, in path order: the survey's that are not a stray's, the
    /// budgets' against `baseline`, and the counts'.
    pub(crate) fn faults(&self, baseline: Option<&Baseline>) -> Vec<Value> {
        let mut faults: Vec<Value> = self
            .answer
            .faults
            .iter()
            .filter(|f| f["fault"] != "unregistered")
            .cloned()
            .collect();
        faults.extend(budget::check(&self.docs, &self.strays, baseline));
        faults.extend(refs::check(self.counted.refs, baseline));
        faults.extend(restates::check(&self.counted.restated, baseline));
        faults.extend(measurements::check(&self.counted.measured, baseline));
        faults.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
        faults
    }

    /// The baseline `held` ratchets to: the budgets' and the counts', the
    /// local refs' from `refs_held` (`None`: each held at its count now).
    pub(crate) fn ratcheted(
        &self,
        held: Option<&Baseline>,
        refs_held: Option<&Baseline>,
    ) -> Baseline {
        let counted = &self.counted;
        let mut next = budget::ratchet(&self.docs, &self.strays, held);
        refs::ratchet(counted.refs, refs_held, &mut next);
        restates::ratchet(&counted.restated, held, &mut next);
        measurements::ratchet(&counted.measured, held, &mut next);
        next
    }

    /// `docfit init`'s adoption: `held` (without one, the budgets held as
    /// they are now), holding each count it holds none of yet.
    pub(crate) fn adopted(&self, held: Option<&Baseline>) -> Baseline {
        let counted = &self.counted;
        let mut next = match held {
            Some(held) => held.clone(),
            None => budget::ratchet(&self.docs, &self.strays, None),
        };
        // A baseline holding no refs has not adopted the check: hold them now.
        if !refs::adopted(&next) {
            refs::ratchet(counted.refs, None, &mut next);
        }
        restates::adopt(&counted.restated, &mut next);
        measurements::adopt(&counted.measured, &mut next);
        next
    }
}

/// The commit that last wrote the baseline beside a `docfit.toml`, and the
/// bytes an agent had to read at it ([`Judged::must_read`] then); none before
/// such a commit, or when that `docfit.toml` does not parse.
pub(crate) fn must_read_then(top: &Path) -> Option<(String, usize)> {
    let (commit, config, sizes) = repo::at_last_change(top, BASELINE, CONFIG)?;
    let paths: Vec<String> = sizes.keys().cloned().collect();
    let answer = documents::check(&documents::config(&config).ok()?, &paths);
    Some((commit, answer.must_read(|f| sizes[f])))
}

/// The text of each of `files` at `top`: one that is a symbolic link, that
/// cannot be read, or that holds a NUL byte in its first 8000 (binary), is
/// not read.
fn texts<'a>(top: &Path, files: impl Iterator<Item = &'a String>) -> Vec<Text> {
    files
        .filter(|f| std::fs::symlink_metadata(top.join(f)).is_ok_and(|m| !m.is_symlink()))
        .filter_map(|f| Some((f, std::fs::read(top.join(f)).ok()?)))
        .filter(|(_, bytes)| !bytes.iter().take(8000).any(|b| *b == 0))
        .map(|(f, bytes)| Text {
            path: f.clone(),
            text: String::from_utf8_lossy(&bytes).into_owned(),
        })
        .collect()
}

/// The local refs in each of `kept`, the text files `config` does not
/// ignore, by its rule.
fn found(config: &Config, kept: &[Text]) -> Found {
    let mut found = Found::new();
    for file in kept {
        let refs = refs::find(&file.text, &config.refs);
        if !refs.is_empty() {
            found.insert(file.path.clone(), refs);
        }
    }
    found
}

/// Every registered document, with its text read from the working tree.
fn read<'a>(
    top: &Path,
    answer: &'a Answer,
) -> Result<Vec<(&'a str, Document, String)>, Vec<Refusal>> {
    let (mut texts, mut refusals) = (Vec::new(), Vec::new());
    for (file, doc) in &answer.documents {
        match tracked_text(top, file) {
            Ok(text) => texts.push((file.as_str(), *doc, text)),
            Err(r) => refusals.push(r),
        }
    }
    if refusals.is_empty() {
        Ok(texts)
    } else {
        Err(refusals)
    }
}

/// Each document read, measured, with its pointers to what git neither
/// tracks nor ignores: git is asked once, for every pointer of them all.
fn measured<'a>(
    top: &Path,
    texts: &[(&'a str, Document, String)],
    tracked: &[String],
) -> Result<Vec<Measured<'a>>, Vec<Refusal>> {
    let tree = Tree::new(tracked);
    let missing: Vec<_> = texts
        .iter()
        .map(|(file, _, text)| pointers::missing(file, text, &tree))
        .collect();
    let wanted = missing.iter().flatten().flat_map(|m| m.paths.clone());
    let ignored: BTreeSet<String> = repo::ignored(top, &wanted.collect())?.into_iter().collect();
    Ok(texts
        .iter()
        .zip(missing)
        .map(|((file, doc, text), missing)| {
            let (size, at) = Size::of(text);
            Measured {
                file,
                budget: doc.budget,
                size,
                at,
                broken: missing
                    .iter()
                    .filter(|m| !m.paths.iter().any(|p| ignored.contains(p)))
                    .map(|m| m.fault(file))
                    .collect(),
            }
        })
        .collect())
}

/// Each tracked Markdown file registered under no kind, measured, with the
/// `unregistered` fault `answer` holds on it.
fn strays<'a>(top: &Path, answer: &'a Answer) -> Result<Vec<Stray<'a>>, Vec<Refusal>> {
    let (mut strays, mut refusals) = (Vec::new(), Vec::new());
    for fault in answer
        .faults
        .iter()
        .filter(|f| f["fault"] == "unregistered")
    {
        let file = fault["file"].as_str().expect("a fault names its file");
        match std::fs::metadata(top.join(file)) {
            Ok(meta) => strays.push(Stray {
                file,
                bytes: meta.len() as usize,
                fault,
            }),
            Err(e) => refusals.push(refusal(
                &format!("cannot read `{file}`, which git tracks: {e}"),
                &format!("restore `{file}`, or `git rm` it"),
            )),
        }
    }
    if refusals.is_empty() {
        Ok(strays)
    } else {
        Err(refusals)
    }
}

/// What `docfit where` reads of a repo, once, to place any of its documents:
/// its survey, the text of every tracked file outside Markdown, and its ADRs.
pub(crate) struct Seen {
    pub(crate) survey: Survey,
    code: Vec<String>,
    adrs: Vec<Adr>,
}

impl Seen {
    pub(crate) fn at(top: &Path) -> Result<Seen, Vec<Refusal>> {
        let survey = Survey::at(top)?;
        let code: Vec<String> = texts(top, survey.tracked.iter().filter(|f| !f.ends_with(".md")))
            .into_iter()
            .map(|t| t.text)
            .collect();
        let adrs = survey
            .answer
            .documents
            .iter()
            .filter(|(_, d)| d.kind == Kind::Adr)
            .map(|(f, _)| tracked_text(top, f).map(|text| Adr::read(f, &text)))
            .collect::<Result<_, _>>()
            .map_err(|r| vec![r])?;
        Ok(Seen { survey, code, adrs })
    }

    /// `docfit where`'s answer on the document `name`, from the repo's top, of `text`.
    pub(crate) fn place(&self, name: &str, text: &str) -> Value {
        let survey = &self.survey;
        let paragraphs = paragraph::split(text);
        let (paths, others): (BTreeSet<String>, BTreeSet<String>) =
            paragraph::candidates(&paragraphs)
                .into_iter()
                .partition(|s| paragraph::is_path(s, &survey.tracked));
        let index = Index::new(&survey.kept);
        let restates = restates::find(name, text, &paragraphs, &survey.catalogs, &index);
        let known = Known {
            words: paragraph::words(others.iter().map(String::as_str), &self.code),
            paths,
            restates: &restates,
            adrs: &self.adrs,
        };
        let placed: Vec<_> = paragraphs
            .iter()
            .map(|p| paragraph::place(p, &known))
            .collect();
        let (mut homes, mut each) = (serde_json::Map::new(), Vec::new());
        for home in Home::ALL {
            let n = placed.iter().filter(|p| p.home == home).count();
            homes.insert(home.name().to_string(), json!(n));
            each.push(format!("{} {n}", home.name()));
        }
        let next = if placed.is_empty() {
            format!("nothing to place: `{name}` has no paragraph")
        } else {
            format!(
                "move each paragraph to its home ({}; `docfit where --help` says what each is), then run `docfit check`",
                each.join(", ")
            )
        };
        let kind = survey.answer.documents.get(name).map(|d| d.kind.name());
        json!({ "file": name, "kind": kind, "paragraphs": placed, "homes": homes, "next": next })
    }
}

/// Each tracked file `catalogs` matches, with its text; every glob must match one.
pub(crate) fn catalogs(
    top: &Path,
    config: &Config,
    tracked: &[String],
) -> Result<Vec<Text>, Vec<Refusal>> {
    let (mut texts, mut refusals) = (Vec::new(), Vec::new());
    for glob in &config.catalogs {
        let files: Vec<&String> = tracked.iter().filter(|f| glob.is_match(f)).collect();
        if files.is_empty() {
            let glob = glob.glob().glob();
            refusals.push(refusal(
                &format!("`{glob}` in {CONFIG}'s `{CATALOGS}` matches no file git tracks"),
                &format!("fix `{glob}` to match the repo's string catalogs, or take it out of `{CATALOGS}`"),
            ));
        }
        for file in files {
            match tracked_text(top, file) {
                Ok(text) => texts.push(Text {
                    path: file.clone(),
                    text,
                }),
                Err(r) => refusals.push(r),
            }
        }
    }
    if refusals.is_empty() {
        Ok(texts)
    } else {
        Err(refusals)
    }
}

/// A tracked file's text.
pub(crate) fn tracked_text(top: &Path, file: &str) -> Result<String, Refusal> {
    std::fs::read_to_string(top.join(file)).map_err(|e| {
        refusal(
            &format!("cannot read `{file}`, which git tracks: {e}"),
            &format!("restore `{file}` as UTF-8 text, or `git rm` it"),
        )
    })
}

/// `text` written to `path` whole or not at all: a temp file beside it, renamed over it.
pub(crate) fn write(path: &Path, text: &str) -> Result<(), Vec<Refusal>> {
    let failed = |e: std::io::Error| {
        vec![refusal(
            &format!("cannot write {}: {e}", path.display()),
            &format!("make {} writable, then run it again", path.display()),
        )]
    };
    let dir = path.parent().expect("a file is in a directory");
    std::fs::create_dir_all(dir).map_err(failed)?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    std::fs::write(&temp, text).map_err(failed)?;
    std::fs::rename(&temp, path).map_err(failed)
}
