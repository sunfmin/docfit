//! Decides `docfit check`: which of a repo's tracked Markdown files
//! `docfit.toml` registers, under which kind. Pure: the files and the
//! config's text arrive as arguments.

use crate::budget::Size;
use crate::kind::Kind;
use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The file a repo declares its documents in, at its top.
pub(crate) const CONFIG: &str = "docfit.toml";
/// The table of `docfit.toml` that maps a glob to a kind.
pub(crate) const TABLE: &str = "documents";
/// The key of `docfit.toml` that lists the globs of the repo's string catalogs.
pub(crate) const CATALOGS: &str = "catalogs";
/// The key of `docfit.toml` that lists the globs of paths that are no document.
pub(crate) const IGNORE: &str = "ignore";
/// What every repo ignores, before its own `ignore` adds to it: the fits'
/// own records.
pub(crate) const IGNORED: [&str; 2] = [".slimfit/**", ".truefit/**"];
/// The directories `docfit init` reads Markdown under as code or a test's
/// data, not a document: a path with one of these as a directory is ignored
/// from that directory down.
pub(crate) const SOURCE: [&str; 6] = [
    "src",
    "tests",
    "test",
    "fixtures",
    "testdata",
    "__fixtures__",
];

/// One fault in `docfit.toml` that stops the check, and its fix.
#[derive(serde::Serialize)]
pub(crate) struct Refusal {
    pub(crate) fault: String,
    pub(crate) fix: String,
}

/// One line of `[documents]`: its glob, its kind, and the measures of the
/// budget it sets itself.
pub(crate) struct Entry {
    matcher: GlobMatcher,
    kind: Kind,
    bytes: Option<usize>,
    paragraph: Option<usize>,
}

/// `docfit.toml`, read: its entries, the globs of its string catalogs, and
/// the globs of what it ignores, [`IGNORED`] among them.
pub(crate) struct Config {
    pub(crate) entries: Vec<Entry>,
    pub(crate) catalogs: Vec<GlobMatcher>,
    ignore: Vec<GlobMatcher>,
}

impl Config {
    /// Whether `file` is ignored: neither a document nor a file to rehome.
    fn ignores(&self, file: &str) -> bool {
        self.ignore.iter().any(|g| g.is_match(file))
    }

    /// The Markdown files of `files` it does not ignore.
    fn markdown<'a>(&'a self, files: &'a [String]) -> impl Iterator<Item = &'a String> {
        files
            .iter()
            .filter(|f| f.ends_with(".md") && !self.ignores(f))
    }

    /// Whether an entry registers `file`.
    fn registers(&self, file: &str) -> bool {
        self.entries.iter().any(|e| e.matcher.is_match(file))
    }
}

/// A registered document: its kind, and its budget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Document {
    pub(crate) kind: Kind,
    pub(crate) budget: Size,
}

/// The check's answer: each document, and each fault with its fix.
pub(crate) struct Answer {
    pub(crate) documents: BTreeMap<String, Document>,
    pub(crate) faults: Vec<Value>,
}

/// An entry's value: a kind's name, or a table that also sets a budget.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Entered {
    Kind(String),
    Budget(Budgeted),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Budgeted {
    kind: String,
    bytes: Option<usize>,
    paragraph: Option<usize>,
}

/// How an entry is written, for a fix.
const SHAPES: &str = "`\"<kind>\"` or `{ kind = \"<kind>\", bytes = <n>, paragraph = <n> }`";

/// `docfit.toml` read, every fault in it at once.
pub(crate) fn config(config: &str) -> Result<Config, Vec<Refusal>> {
    let table: toml::Table = config.parse().map_err(|e: toml::de::Error| {
        vec![Refusal {
            fault: format!("{CONFIG} is not TOML: {}", e.message()),
            fix: format!("fix {CONFIG}"),
        }]
    })?;
    let Some(toml::Value::Table(documents)) = table.get(TABLE) else {
        return Err(vec![Refusal {
            fault: format!("{CONFIG} has no `[{TABLE}]` table"),
            fix: format!(
                "add a `[{TABLE}]` table to {CONFIG}, a line `\"<glob>\" = \"<kind>\"` per document"
            ),
        }]);
    };
    let (mut entries, mut refusals) = (Vec::new(), Vec::new());
    let catalogs = globs(
        CATALOGS,
        table.get(CATALOGS),
        "string catalog",
        &mut refusals,
    );
    let ignore = IGNORED
        .iter()
        .map(|g| matcher(g).expect("a glob"))
        .chain(globs(
            IGNORE,
            table.get(IGNORE),
            "path that is no document",
            &mut refusals,
        ))
        .collect();
    for (glob, value) in documents {
        let (kind, bytes, paragraph) = match value.clone().try_into() {
            Ok(Entered::Kind(name)) => (name, None, None),
            Ok(Entered::Budget(b)) => (b.kind, b.bytes, b.paragraph),
            Err(_) => {
                refusals.push(Refusal {
                    fault: format!("`{glob}` = `{value}` is not an entry docfit reads"),
                    fix: format!(
                        "give `{glob}` one of the kinds {}, as {SHAPES}",
                        Kind::names(&Kind::ALL)
                    ),
                });
                continue;
            }
        };
        let kind = Kind::named(&kind).ok_or(kind);
        match (kind, matcher(glob)) {
            (Ok(kind), Ok(matcher)) => entries.push(Entry {
                matcher,
                kind,
                bytes,
                paragraph,
            }),
            (Err(name), _) => refusals.push(Refusal {
                fault: format!("`{glob}` names the kind `{name}`, which docfit does not know"),
                fix: format!("give `{glob}` one of the kinds {}", Kind::names(&Kind::ALL)),
            }),
            (_, Err(e)) => refusals.push(Refusal {
                fault: format!("`{glob}` is not a glob: {}", e.kind()),
                fix: format!("fix `{glob}` in {CONFIG}"),
            }),
        }
    }
    if refusals.is_empty() {
        Ok(Config {
            entries,
            catalogs,
            ignore,
        })
    } else {
        Err(refusals)
    }
}

/// A glob over paths from the repo's top: `*` and `?` within one directory.
fn matcher(glob: &str) -> Result<GlobMatcher, globset::Error> {
    GlobBuilder::new(glob)
        .literal_separator(true)
        .build()
        .map(|g| g.compile_matcher())
}

/// The globs `key` lists, each one a glob per `what`; none when it is absent.
fn globs(
    key: &str,
    value: Option<&toml::Value>,
    what: &str,
    refusals: &mut Vec<Refusal>,
) -> Vec<GlobMatcher> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(globs) = value.clone().try_into::<Vec<String>>() else {
        refusals.push(Refusal {
            fault: format!("`{key}` = `{value}` is not a list of globs"),
            fix: format!("write `{key} = [\"<glob>\", ...]` in {CONFIG}, a glob per {what}"),
        });
        return Vec::new();
    };
    let mut matchers = Vec::new();
    for glob in &globs {
        match matcher(glob) {
            Ok(m) => matchers.push(m),
            Err(e) => refusals.push(Refusal {
                fault: format!("`{glob}` in `{key}` is not a glob: {}", e.kind()),
                fix: format!("fix `{glob}` in {CONFIG}'s `{key}`"),
            }),
        }
    }
    matchers
}

/// The check of `files`, the repo's tracked paths, against `config`: each
/// Markdown file it does not ignore.
pub(crate) fn check(config: &Config, files: &[String]) -> Answer {
    let mut answer = Answer {
        documents: BTreeMap::new(),
        faults: Vec::new(),
    };
    for file in config.markdown(files) {
        let matching = || config.entries.iter().filter(|e| e.matcher.is_match(file));
        let mut kinds: Vec<Kind> = Vec::new();
        for entry in matching() {
            if !kinds.contains(&entry.kind) {
                kinds.push(entry.kind);
            }
        }
        match kinds.as_slice() {
            [kind] => {
                let set = |measure: fn(&Entry) -> Option<usize>, default: usize| {
                    matching().filter_map(measure).min().unwrap_or(default)
                };
                let budget = Size {
                    bytes: set(|e| e.bytes, kind.budget().bytes),
                    paragraph: set(|e| e.paragraph, kind.budget().paragraph),
                };
                answer.documents.insert(
                    file.clone(),
                    Document {
                        kind: *kind,
                        budget,
                    },
                );
            }
            [] => {
                let candidates = Kind::candidates(file);
                let kind = if let [one] = candidates.as_slice() {
                    one.name()
                } else {
                    "<kind>"
                };
                let mut fix =
                    format!("add `\"{file}\" = \"{kind}\"` under `[{TABLE}]` in {CONFIG}");
                if candidates.len() > 1 {
                    fix.push_str(&format!(", <kind> one of {}", Kind::names(&candidates)));
                }
                answer
                    .faults
                    .push(fault(file, "unregistered", &candidates, fix));
            }
            several => {
                let fix = format!(
                    "register `{file}` under one of {} in {CONFIG}, not all",
                    Kind::names(several)
                );
                answer.faults.push(fault(file, "two_kinds", several, fix));
            }
        }
    }
    answer
}

fn fault(file: &str, fault: &str, kinds: &[Kind], fix: String) -> Value {
    json!({ "file": file, "fault": fault, "kinds": kinds.iter().map(|k| k.name()).collect::<Vec<_>>(), "fix": fix })
}

/// The `docfit.toml` a repo starts with, before `docfit init` adds its lines.
const STARTED: &str =
    "# This repo's documents, each under its kind (`docfit check --help`).\n[documents]\n";

/// What `docfit init` adopts: `docfit.toml`'s text, each file it registered
/// with its kind, and each glob it added to `ignore` with the files it ignores.
pub(crate) struct Adopted {
    pub(crate) text: String,
    pub(crate) registered: BTreeMap<String, Kind>,
    pub(crate) ignored: BTreeMap<String, Vec<String>>,
}

/// `config` (`None`: the repo has no `docfit.toml` yet) with a glob added to
/// `ignore` for each directory of [`SOURCE`] holding Markdown of `files` that
/// is neither ignored nor registered, unless an entry registers a file under
/// it; then a line added for each file that is not registered and whose place
/// says its kind. Every other line already there is kept as it is, so on a
/// config that registers them all it answers `config` itself.
pub(crate) fn adopt(config: Option<&str>, files: &[String]) -> Result<Adopted, Vec<Refusal>> {
    let mut text = config.unwrap_or(STARTED).to_string();
    if text
        .parse::<toml::Table>()
        .is_ok_and(|t| !t.contains_key(TABLE))
    {
        text = format!("{}\n\n[{TABLE}]\n", text.trim_end());
    }
    let ignored = sources(&self::config(&text)?, files);
    if !ignored.is_empty() {
        text = ignore_more(&text, ignored.keys())?;
    }
    let answer = check(&self::config(&text)?, files);
    let added: BTreeMap<String, Kind> = answer
        .faults
        .iter()
        .filter(|f| f["fault"] == "unregistered")
        .filter_map(|f| {
            let file = f["file"].as_str()?;
            Some((file.to_string(), Kind::conventional(file)?))
        })
        .collect();
    if !added.is_empty() {
        text = register_more(&text, &added)?;
    }
    Ok(Adopted {
        text,
        registered: added,
        ignored,
    })
}

/// `text` with a line under `[documents]` for each of `added`, after its last.
fn register_more(text: &str, added: &BTreeMap<String, Kind>) -> Result<String, Vec<Refusal>> {
    let mut lines: Vec<&str> = text.lines().collect();
    let header = format!("[{TABLE}]");
    let Some(at) = lines
        .iter()
        .position(|l| l.split('#').next().unwrap_or("").trim() == header)
    else {
        return Err(vec![Refusal {
            fault: format!("{CONFIG} has `{TABLE}` but not as a `{header}` table header"),
            fix: format!("write `{TABLE}` in {CONFIG} as a `{header}` table, a line per glob"),
        }]);
    };
    let mut end = lines[at + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |n| at + 1 + n);
    while end > at + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let new: Vec<String> = added
        .iter()
        .map(|(file, kind)| format!("{} = \"{}\"", toml::Value::from(file.as_str()), kind.name()))
        .collect();
    lines.splice(end..end, new.iter().map(String::as_str));
    Ok(lines.join("\n") + "\n")
}

/// For each directory of [`SOURCE`] (the first in its path) holding Markdown
/// of `files` that `config` neither ignores nor registers, its glob and those
/// files; but not a directory under which an entry registers a file.
fn sources(config: &Config, files: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut globs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in config.markdown(files).filter(|f| !config.registers(f)) {
        let dirs: Vec<&str> = file.split('/').collect();
        let dirs = &dirs[..dirs.len() - 1];
        if let Some(at) = dirs.iter().position(|d| SOURCE.contains(d)) {
            let glob = format!("{}/**", dirs[..=at].join("/"));
            globs.entry(glob).or_default().push(file.clone());
        }
    }
    globs.retain(|glob, _| {
        let under = matcher(glob).expect("a glob");
        !config
            .markdown(files)
            .any(|f| config.registers(f) && under.is_match(f))
    });
    globs
}

/// `text` with `globs` added to its `ignore`: the list written again, on one
/// line, where it was; or, without one, a line before the first table (and
/// the comments just above it).
fn ignore_more<'a>(
    text: &str,
    globs: impl Iterator<Item = &'a String>,
) -> Result<String, Vec<Refusal>> {
    let table: toml::Table = text.parse().expect("read by config");
    let mut all: Vec<toml::Value> = match table.get(IGNORE) {
        Some(toml::Value::Array(had)) => had.clone(),
        _ => Vec::new(),
    };
    all.extend(globs.map(|g| toml::Value::from(g.as_str())));
    let line = format!("{IGNORE} = {}", toml::Value::Array(all));
    let mut lines: Vec<&str> = text.lines().collect();
    let header = |l: &&str| l.trim_start().starts_with('[');
    let (at, new) = if table.contains_key(IGNORE) {
        let key = |l: &&str| {
            l.strip_prefix(IGNORE)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        };
        let start = lines.iter().take_while(|l| !header(l)).position(key);
        let end = start.and_then(|start| {
            (start + 1..=lines.len()).find(|&end| {
                lines[start..end]
                    .join("\n")
                    .parse::<toml::Table>()
                    .is_ok_and(|t| t.contains_key(IGNORE))
            })
        });
        let (Some(start), Some(end)) = (start, end) else {
            return Err(vec![Refusal {
                fault: format!(
                    "{CONFIG} has `{IGNORE}` but not as a line `{IGNORE} = [...]` before its first table"
                ),
                fix: format!(
                    "write `{IGNORE} = [\"<glob>\", ...]` in {CONFIG} at the start of a line, before its first table"
                ),
            }]);
        };
        (start..end, vec![line.as_str()])
    } else {
        let mut at = lines.iter().position(header).unwrap_or(0);
        while at > 0 && lines[at - 1].trim_start().starts_with('#') {
            at -= 1;
        }
        (at..at, vec![line.as_str(), ""])
    };
    lines.splice(at, new);
    Ok(lines.join("\n") + "\n")
}
