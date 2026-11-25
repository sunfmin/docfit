//! Decides `docfit where`: a document cut into paragraphs, what can be seen
//! in each mechanically, and the home that suggests. Pure: the document's
//! text, what the repo holds of each candidate, and its ADRs arrive as
//! arguments.

use serde::Serialize;
use std::collections::BTreeSet;

/// One paragraph: the line it starts on (from 1), its lines joined on
/// spaces, and whether it is a fenced block, in which nothing is read.
pub(crate) struct Paragraph {
    line: usize,
    text: String,
    fenced: bool,
}

/// Where a paragraph's content belongs, as what was seen in it suggests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Home {
    Keep,
    Adr,
    Delete,
    Evidence,
    Replace,
    Judge,
}

impl Home {
    /// In the order a paragraph's home is chosen: the first that applies.
    pub(crate) const ALL: [Home; 6] = [
        Home::Keep,
        Home::Adr,
        Home::Delete,
        Home::Evidence,
        Home::Replace,
        Home::Judge,
    ];

    /// The home's name, as the answer spells it; what in a paragraph
    /// suggests it; and what to do with the paragraph.
    fn facts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Home::Keep => (
                "keep",
                "`pointer`",
                "keep it, in a document of kind `agent` or `readme`",
            ),
            Home::Adr => (
                "adr",
                "a `decision`",
                "move it to an ADR: the one it names, if it names one that exists, else a new one",
            ),
            Home::Delete => (
                "delete",
                "`code` or `strings`",
                "delete it: the code, the catalog and the screen tests are its home",
            ),
            Home::Evidence => (
                "evidence",
                "`evidence`",
                "move it to the ADR of the decision beside it, with the command that measures it, if there is one; else to a comment on the issue it came from",
            ),
            Home::Replace => (
                "replace",
                "`local_refs`",
                "replace each reference with what it points at: the person's words with who, date and where; or the command and the number it printed. Only this host can open the local tracker",
            ),
            Home::Judge => ("judge", "none of these", "for the agent to judge"),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        self.facts().0
    }

    /// The homes as a Markdown table, for the manual.
    pub(crate) fn table() -> String {
        let mut table = String::from("| home | when it holds | what to do |\n|---|---|---|\n");
        for home in Home::ALL {
            let (name, when, what) = home.facts();
            table.push_str(&format!("| `{name}` | {when} | {what} |\n"));
        }
        table.trim_end().to_string()
    }
}

/// An ADR the repo has: its path, its number (the digits its file name
/// starts with), and its title in lower case.
pub(crate) struct Adr {
    path: String,
    number: Option<u32>,
    title: String,
}

impl Adr {
    /// The ADR at `path`, its title its first `# ` heading less any number.
    pub(crate) fn read(path: &str, text: &str) -> Adr {
        let name = path.rsplit('/').next().unwrap_or(path);
        let number = digits(name).parse().ok();
        let heading = text
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .unwrap_or("");
        let heading = heading.trim().to_lowercase();
        let title = heading.strip_prefix("adr").unwrap_or(&heading);
        let title = title.trim_start_matches(|c: char| c.is_ascii_digit() || " .:-–—".contains(c));
        Adr {
            path: path.to_string(),
            number,
            title: title.to_string(),
        }
    }
}

/// What the repo holds of a document's candidates: the code spans that are
/// a path git tracks, those found as a word outside Markdown, the quoted
/// strings its catalogs hold, and its ADRs.
pub(crate) struct Known<'a> {
    pub(crate) paths: BTreeSet<String>,
    pub(crate) words: BTreeSet<String>,
    pub(crate) strings: BTreeSet<String>,
    pub(crate) adrs: &'a [Adr],
}

/// Candidates seen, distinct, in the order the paragraph has them.
#[derive(Serialize)]
pub(crate) struct Found {
    count: usize,
    found: Vec<String>,
}

impl Found {
    fn of(seen: impl Iterator<Item = String>, known: impl Fn(&str) -> bool) -> Found {
        let mut found: Vec<String> = Vec::new();
        for s in seen.filter(|s| known(s)) {
            if !found.contains(&s) {
                found.push(s);
            }
        }
        Found {
            count: found.len(),
            found,
        }
    }
}

/// A paragraph placed: where it is, what was seen in it, and its home.
#[derive(Serialize)]
pub(crate) struct Placed {
    line: usize,
    text: String,
    code: Found,
    strings: Found,
    evidence: Vec<String>,
    local_refs: Vec<String>,
    decision: Vec<&'static str>,
    pointer: bool,
    pub(crate) home: Home,
    /// With home `adr`: the ADR the paragraph names, or null for a new one.
    #[serde(skip_serializing_if = "Option::is_none")]
    adr: Option<Option<String>>,
}

/// `text` cut into paragraphs: a list item and a table row are each one, a
/// fenced block is one; a heading, a table's header and its separator are none.
pub(crate) fn split(text: &str) -> Vec<Paragraph> {
    let mut lines = text.lines().enumerate().peekable();
    let (mut out, mut open): (Vec<Paragraph>, Option<Paragraph>) = (Vec::new(), None);
    while let Some((i, line)) = lines.next() {
        let t = line.trim();
        let starts = |fenced| Paragraph {
            line: i + 1,
            text: t.to_string(),
            fenced,
        };
        if let Some(fence) = ["```", "~~~"].into_iter().find(|f| t.starts_with(f)) {
            out.extend(open.take());
            let mut block = starts(true);
            for (_, l) in lines.by_ref() {
                block.text.push('\n');
                block.text.push_str(l);
                if l.trim().starts_with(fence) {
                    break;
                }
            }
            out.push(block);
        } else if t.is_empty() || t.trim_start_matches('#').starts_with(' ') {
            out.extend(open.take());
        } else if t.starts_with('|') {
            out.extend(open.take());
            if !separator(t) && !lines.peek().is_some_and(|(_, n)| separator(n.trim())) {
                out.push(starts(false));
            }
        } else if let Some(p) = open.as_mut().filter(|_| !item(t)) {
            p.text.push(' ');
            p.text.push_str(t);
        } else {
            out.extend(open.replace(starts(false)));
        }
    }
    out.extend(open);
    out
}

/// The longest paragraph of `text` outside fenced code, in characters, its
/// lines joined on spaces; and the line (from 1) it starts on. `(0, 0)` when
/// `text` has none.
pub(crate) fn longest(text: &str) -> (usize, usize) {
    split(text)
        .iter()
        .filter(|p| !p.fenced)
        .map(|p| (p.text.chars().count(), p.line))
        .fold((0, 0), |best, p| if p.0 > best.0 { p } else { best })
}

/// The ASCII digits `s` starts with.
fn digits(s: &str) -> &str {
    &s[..s.bytes().take_while(u8::is_ascii_digit).count()]
}

fn separator(t: &str) -> bool {
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| "|-: ".contains(c))
}

fn item(t: &str) -> bool {
    let n = digits(t).len();
    ["- ", "* ", "+ "].iter().any(|m| t.starts_with(m))
        || (n > 0 && [". ", ") "].iter().any(|m| t[n..].starts_with(m)))
}

/// Every code span that could be code, and every quoted string, in
/// `paragraphs`: what the repo is asked about.
pub(crate) fn candidates(paragraphs: &[Paragraph]) -> (BTreeSet<String>, BTreeSet<String>) {
    let read = paragraphs.iter().filter(|p| !p.fenced);
    let spans = read.clone().flat_map(|p| tokens(&p.text)).collect();
    let strings = read.flat_map(|p| quoted(&prose(&p.text))).collect();
    (spans, strings)
}

/// The `tokens` found as a whole word in `texts`, as `git grep -w` finds
/// them: neither side touching a letter, a digit or `_`.
pub(crate) fn words<'a>(
    tokens: impl Iterator<Item = &'a str>,
    texts: &[String],
) -> BTreeSet<String> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let (plain, compound): (Vec<&str>, Vec<&str>) = tokens.partition(|t| t.chars().all(word));
    let plain: BTreeSet<&str> = plain.into_iter().collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for text in texts {
        seen.extend(text.split(|c| !word(c)).filter(|w| plain.contains(w)));
    }
    let whole = |text: &str, t: &str| {
        text.match_indices(t)
            .any(|(i, _)| !text[..i].ends_with(word) && !text[i + t.len()..].starts_with(word))
    };
    let mut found: BTreeSet<String> = seen.into_iter().map(str::to_string).collect();
    found.extend(
        compound
            .into_iter()
            .filter(|t| texts.iter().any(|text| whole(text, t)))
            .map(str::to_string),
    );
    found
}

/// Whether `token` is a path git tracks, as a file or a directory.
pub(crate) fn is_path(token: &str, tracked: &[String]) -> bool {
    let t = token.trim_end_matches('/');
    tracked
        .iter()
        .any(|f| f == t || f.strip_prefix(t).is_some_and(|r| r.starts_with('/')))
}

/// `p` placed against what the repo holds. The home is the first that
/// applies: a pointer is kept, a decision goes to an ADR, code or strings
/// are deleted, evidence goes to an ADR or an issue's comment, a local
/// tracker's reference is replaced, and the rest is for the agent.
pub(crate) fn place(p: &Paragraph, known: &Known) -> Placed {
    let read = if p.fenced { "" } else { &p.text };
    let prose = prose(read);
    let code = Found::of(tokens(read), |t| {
        known.paths.contains(t) || known.words.contains(t)
    });
    let strings = Found::of(quoted(&prose).into_iter(), |s| known.strings.contains(s));
    let (evidence, local_refs) = (measures(&prose), issues(&prose));
    let decision = decision(&prose);
    let pointer = pointer(read, &known.paths);
    let home = if pointer {
        Home::Keep
    } else if !decision.is_empty() {
        Home::Adr
    } else if code.count + strings.count > 0 {
        Home::Delete
    } else if !evidence.is_empty() {
        Home::Evidence
    } else if !local_refs.is_empty() {
        Home::Replace
    } else {
        Home::Judge
    };
    let adr = (home == Home::Adr).then(|| named(read, known.adrs).map(|a| a.path.clone()));
    Placed {
        line: p.line,
        text: excerpt(&p.text),
        code,
        strings,
        evidence,
        local_refs,
        decision,
        pointer,
        home,
        adr,
    }
}

/// The paragraph's start, to find it by.
fn excerpt(text: &str) -> String {
    let first = text.lines().next().unwrap_or("");
    let mut out: String = first.chars().take(80).collect();
    if out.len() < text.len() {
        out.push('…');
    }
    out
}

/// The code spans that could be code: one word of letters, digits and
/// `_.:/-`, a trailing `()` and a leading `./` dropped.
fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('`').skip(1).step_by(2).filter_map(token)
}

fn token(span: &str) -> Option<String> {
    let t = span.trim();
    let t = t.strip_suffix("()").unwrap_or(t);
    let t = t.strip_prefix("./").unwrap_or(t);
    let word = t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_.:/-".contains(c));
    (t.len() >= 2 && word && t.chars().any(|c| c.is_ascii_alphabetic())).then(|| t.to_string())
}

/// The text outside its code spans.
fn prose(text: &str) -> String {
    text.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

const QUOTES: [(char, char); 4] = [('"', '"'), ('“', '”'), ('「', '」'), ('『', '』')];

/// The strings quoted in `prose`, each at most 100 characters.
fn quoted(prose: &str) -> Vec<String> {
    let (mut out, mut rest) = (Vec::new(), prose);
    while let Some((at, (open, close))) = rest
        .char_indices()
        .find_map(|(i, c)| QUOTES.iter().find(|q| q.0 == c).map(|q| (i, *q)))
    {
        let after = &rest[at + open.len_utf8()..];
        let Some(end) = after.find(close) else {
            rest = after;
            continue;
        };
        let s = after[..end].trim();
        if !s.is_empty() && s.chars().count() <= 100 {
            out.push(s.to_string());
        }
        rest = &after[end + close.len_utf8()..];
    }
    out
}

/// References to the local tracker's issues: `#123`, and `<checkout>#123`.
fn issues(prose: &str) -> Vec<String> {
    let name = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut out = Vec::new();
    for (i, _) in prose.match_indices('#') {
        let n = digits(&prose[i + 1..]).len();
        let end = i + 1 + n;
        if n == 0 || prose[end..].starts_with(|c: char| c.is_ascii_alphanumeric()) {
            continue;
        }
        let start = prose[..i].rfind(|c| !name(c)).map_or(0, |j| {
            j + prose[j..].chars().next().map_or(1, char::len_utf8)
        });
        out.push(prose[start..end].to_string());
    }
    out
}

/// Units a measured number is written with, longest first where one starts
/// another.
const UNITS: [&str; 36] = [
    "milliseconds",
    "seconds",
    "minutes",
    "second",
    "minute",
    "hours",
    "bytes",
    "hour",
    "days",
    "day",
    "fps",
    "KiB",
    "MiB",
    "GiB",
    "TiB",
    "KB",
    "MB",
    "GB",
    "TB",
    "ms",
    "µs",
    "us",
    "ns",
    "px",
    "min",
    "sec",
    "毫秒",
    "分钟",
    "小时",
    "秒",
    "天",
    "倍",
    "×",
    "s",
    "h",
    "B",
];

/// Numbers with units: `1.5 s`, `63 MiB`, `70 分钟`; a bare percentage is
/// not one (`100% offline`).
fn measures(prose: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = prose;
    let mut prev = ' ';
    while let Some(c) = rest.chars().next() {
        let next = &rest[c.len_utf8()..];
        if !c.is_ascii_digit() || prev.is_ascii_alphanumeric() || ".#_".contains(prev) {
            (prev, rest) = (c, next);
            continue;
        }
        let number = rest
            .char_indices()
            .find(|&(i, d)| {
                !(d.is_ascii_digit()
                    || (".,".contains(d)
                        && rest[i + 1..].starts_with(|n: char| n.is_ascii_digit())))
            })
            .map_or(rest.len(), |(i, _)| i);
        let after = &rest[number..];
        let unit_at = after.strip_prefix(' ').unwrap_or(after);
        let unit = UNITS.iter().find(|u| {
            unit_at.strip_prefix(**u).is_some_and(|tail| {
                let joined = if u.ends_with(|c: char| c.is_ascii_alphabetic()) {
                    tail.starts_with(|c: char| c.is_ascii_alphanumeric())
                } else {
                    tail.starts_with(|c: char| c.is_ascii_digit())
                };
                !joined
            })
        });
        let end = match unit {
            Some(u) => {
                let end = rest.len() - unit_at.len() + u.len();
                out.push(rest[..end].to_string());
                end
            }
            None => number,
        };
        prev = rest[..end].chars().last().unwrap_or(' ');
        rest = &rest[end..];
    }
    out
}

const CHOICE: [&str; 5] = ["because", "chose", "choose", "因为", "选"];
const ALTERNATIVE: [&str; 4] = ["instead of", "rather than", "而不是", "而非"];

/// The markers of a decision: a choice or reason beside an alternative,
/// both or none.
fn decision(prose: &str) -> Vec<&'static str> {
    let low = prose.to_lowercase();
    let seen = |markers: &[&'static str]| -> Vec<&'static str> {
        markers
            .iter()
            .copied()
            .filter(|m| low.contains(m))
            .collect()
    };
    let (choice, alternative) = (seen(&CHOICE), seen(&ALTERNATIVE));
    if choice.is_empty() || alternative.is_empty() {
        return Vec::new();
    }
    [choice, alternative].concat()
}

/// A link, or a code span that is a path git tracks.
const TARGET: char = '\u{1}';

/// Whether every sentence of `text` names where something is and little
/// else: a link or a tracked path, a word of [`PLACES`] unless it is only
/// those, and at most 8 other words (a CJK character a half).
fn pointer(text: &str, paths: &BTreeSet<String>) -> bool {
    let mut marked = String::new();
    for (n, segment) in links(text).split('`').enumerate() {
        if n % 2 == 0 {
            marked.push_str(segment);
        } else if token(segment).is_some_and(|t| paths.contains(&t)) {
            marked.push(TARGET);
        } else {
            marked.push_str(" w ");
        }
    }
    let sentences = sentences(&marked);
    !sentences.is_empty()
        && sentences.iter().all(|s| {
            let halves: usize = s
                .split_whitespace()
                .map(|w| {
                    let ascii = if w.chars().any(|c| c.is_ascii_alphanumeric()) {
                        2
                    } else {
                        0
                    };
                    ascii
                        + w.chars()
                            .filter(|c| !c.is_ascii() && c.is_alphabetic())
                            .count()
                })
                .sum();
            s.contains(TARGET) && halves <= 16 && (halves == 0 || places(s))
        })
}

/// The words that say where something is.
const PLACES: [&str; 9] = ["see", "in", "at", "under", "lives", "见", "在", "位于", "→"];

/// Whether `sentence` holds a word of [`PLACES`], an English one whole.
fn places(sentence: &str) -> bool {
    let low = sentence.to_lowercase();
    let words: Vec<&str> = low.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    PLACES.iter().any(|p| {
        if p.is_ascii() {
            words.contains(p)
        } else {
            low.contains(p)
        }
    })
}

/// `text` with each link `[text](target)` a target mark, code in its text
/// included.
fn links(text: &str) -> String {
    let (mut out, mut rest) = (String::new(), text);
    while let Some(open) = rest.find('[') {
        let Some(end) = rest[open..]
            .find("](")
            .and_then(|c| rest[open + c..].find(')').map(|e| open + c + e))
        else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push(TARGET);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `text`'s sentences, each with a word or a target in it.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let at_end = chars.peek().is_none_or(|(_, n)| n.is_whitespace());
        if "。！？；".contains(c) || (".!?".contains(c) && at_end) {
            out.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&text[start..]);
    out.into_iter()
        .map(str::trim)
        .filter(|s| s.chars().any(|c| c.is_alphanumeric() || c == TARGET))
        .collect()
}

/// The ADR `text` names, by its number (`ADR 0002`, `adr/0002-…`) or its title.
fn named<'a>(text: &str, adrs: &'a [Adr]) -> Option<&'a Adr> {
    let low = text.to_lowercase();
    let numbers: Vec<u32> = low
        .match_indices("adr")
        .filter_map(|(i, _)| {
            let rest = low[i + 3..].trim_start_matches([' ', '-', '#', '/', '_', ':']);
            digits(rest).parse().ok()
        })
        .collect();
    adrs.iter().find(|a| {
        a.number.is_some_and(|n| numbers.contains(&n))
            || (a.title.chars().count() >= 6 && low.contains(&a.title))
    })
}
