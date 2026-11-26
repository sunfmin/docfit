//! What docfit reads of a repo: its `docfit.toml`, the documents it
//! registers, their text, and the text files and catalogs the checks count
//! in; and how what is read is judged against the baseline.

use crate::budget::{self, BASELINE, Baseline, Measured, Size, Stray};
use crate::cli::refusal;
use crate::documents::{self, Answer, CATALOGS, CONFIG, Config, Document, Refusal, TABLE};
use crate::hot::Hot;
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

/// The text of each of `files` at `top`: one that is a symbolic link, that
/// cannot be read, or that holds a NUL byte in its first 8000 (binary), is
/// not read.
pub(crate) fn texts<'a>(top: &Path, files: impl Iterator<Item = &'a String>) -> Vec<Text> {
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
pub(crate) fn found(config: &Config, kept: &[Text]) -> Found {
    let mut found = Found::new();
    for file in kept {
        let refs = refs::find(&file.text, &config.refs);
        if !refs.is_empty() {
            found.insert(file.path.clone(), refs);
        }
    }
    found
}

/// What `docfit check`, `ratchet` and `init` read of a repo beyond its
/// documents: its text files `docfit.toml` does not ignore, the local refs in
/// them, and its string catalogs.
pub(crate) struct Read {
    pub(crate) kept: Vec<Text>,
    pub(crate) found: Found,
    pub(crate) catalogs: Vec<Text>,
}

impl Read {
    pub(crate) fn at(
        top: &Path,
        config: &Config,
        tracked: &[String],
    ) -> Result<Read, Vec<Refusal>> {
        let kept = texts(top, config.kept(tracked));
        Ok(Read {
            found: found(config, &kept),
            catalogs: catalogs(top, config, tracked)?,
            kept,
        })
    }

    /// What is counted in the files: the local refs, and what each document
    /// `answer` registers restates, against the other text files and the
    /// catalogs, and measures.
    pub(crate) fn counted(&self, answer: &Answer) -> Counted<'_> {
        let index = Index::new(&self.kept);
        let (mut restated, mut measured) = (BTreeMap::new(), measurements::Found::new());
        for t in &self.kept {
            let Some(doc) = answer.documents.get(&t.path) else {
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

impl Counted<'_> {
    /// `next` holding each count it holds none of yet: `docfit init`'s
    /// adoption.
    pub(crate) fn adopt(&self, next: &mut Baseline) {
        // A baseline holding no refs has not adopted the check: hold them now.
        if !refs::adopted(next) {
            refs::ratchet(self.refs, None, next);
        }
        restates::adopt(&self.restated, next);
        measurements::adopt(&self.measured, next);
    }
}

/// The baseline `held` ratchets to: the budgets' and the counts', the local
/// refs' from `refs_held` (`None`: each held at its count now).
pub(crate) fn ratcheted(
    docs: &[Measured],
    strays: &[Stray],
    counted: &Counted,
    held: Option<&Baseline>,
    refs_held: Option<&Baseline>,
) -> Baseline {
    let mut next = budget::ratchet(docs, strays, held);
    refs::ratchet(counted.refs, refs_held, &mut next);
    restates::ratchet(&counted.restated, held, &mut next);
    measurements::ratchet(&counted.measured, held, &mut next);
    next
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

/// The documents registered at `top`, the paths git tracks, and `docfit.toml` read.
pub(crate) fn registered_at(top: &Path) -> Result<(Answer, Vec<String>, Config), Vec<Refusal>> {
    registered(top, &written(top)?)
}

/// The documents `text`, a `docfit.toml`, registers at `top`, the paths git
/// tracks, and `text` read.
pub(crate) fn registered(
    top: &Path,
    text: &str,
) -> Result<(Answer, Vec<String>, Config), Vec<Refusal>> {
    let config = documents::config(text)?;
    let tracked = repo::tracked(top)?;
    let answer = documents::check(&config, &tracked);
    Ok((answer, tracked, config))
}

/// Every registered document, with its text read from the working tree.
pub(crate) fn read<'a>(
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
pub(crate) fn measured<'a>(
    top: &Path,
    texts: &'a [(&'a str, Document, String)],
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
pub(crate) fn strays<'a>(top: &Path, answer: &'a Answer) -> Result<Vec<Stray<'a>>, Vec<Refusal>> {
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

/// Every fault, in path order: `answer`'s that are not a stray's, those
/// `budget::check` finds in `docs` and `strays` against `baseline`, and
/// those of `counted`.
pub(crate) fn faults(
    answer: &Answer,
    docs: &[Measured],
    strays: &[Stray],
    counted: &Counted,
    baseline: Option<&Baseline>,
) -> Vec<Value> {
    let mut faults: Vec<Value> = answer
        .faults
        .iter()
        .filter(|f| f["fault"] != "unregistered")
        .cloned()
        .collect();
    faults.extend(budget::check(docs, strays, baseline));
    faults.extend(refs::check(counted.refs, baseline));
    faults.extend(restates::check(&counted.restated, baseline));
    faults.extend(measurements::check(&counted.measured, baseline));
    faults.sort_by(|a, b| a["file"].as_str().cmp(&b["file"].as_str()));
    faults
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

/// What `docfit where` reads of a repo, once, to place any of its documents:
/// its registered documents, the paths git tracks, the text of every tracked
/// file outside Markdown, its string catalogs, and its ADRs; and, for `docfit
/// next`, when `docfit.toml` holds a document hot.
pub(crate) struct Seen {
    pub(crate) answer: Answer,
    pub(crate) tracked: Vec<String>,
    pub(crate) code: Vec<String>,
    pub(crate) read: Read,
    pub(crate) adrs: Vec<Adr>,
    pub(crate) hot: Hot,
    pub(crate) refs: refs::Rule,
}

impl Seen {
    pub(crate) fn at(top: &Path) -> Result<Seen, Vec<Refusal>> {
        let (answer, tracked, config) = registered_at(top)?;
        let code: Vec<String> = texts(top, tracked.iter().filter(|f| !f.ends_with(".md")))
            .into_iter()
            .map(|t| t.text)
            .collect();
        let read = Read::at(top, &config, &tracked)?;
        let adrs = answer
            .documents
            .iter()
            .filter(|(_, d)| d.kind == Kind::Adr)
            .map(|(f, _)| tracked_text(top, f).map(|text| Adr::read(f, &text)))
            .collect::<Result<_, _>>()
            .map_err(|r| vec![r])?;
        Ok(Seen {
            answer,
            tracked,
            code,
            read,
            adrs,
            hot: config.hot,
            refs: config.refs,
        })
    }

    /// `docfit where`'s answer on the document `name`, from the repo's top, of `text`.
    pub(crate) fn place(&self, name: &str, text: &str) -> Value {
        let paragraphs = paragraph::split(text);
        let (paths, others): (BTreeSet<String>, BTreeSet<String>) =
            paragraph::candidates(&paragraphs)
                .into_iter()
                .partition(|s| paragraph::is_path(s, &self.tracked));
        let index = Index::new(&self.read.kept);
        let restates = restates::find(name, text, &paragraphs, &self.read.catalogs, &index);
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
        let kind = self.answer.documents.get(name).map(|d| d.kind.name());
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
