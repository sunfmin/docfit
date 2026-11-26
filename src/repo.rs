//! The repo, read through git's own CLI: the only place that builds its
//! command lines.

use crate::cli::refusal;
use crate::documents::Refusal;
use crate::hot::Commit;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, Vec<Refusal>> {
    fed(dir, args, Vec::new(), &[0])
}

/// `git <args>` in `dir`, given `input` on stdin: its stdout, when it exits
/// with one of `ok`.
fn fed(dir: &Path, args: &[&str], input: Vec<u8>, ok: &[i32]) -> Result<Vec<u8>, Vec<Refusal>> {
    let cannot = |e: std::io::Error| {
        vec![refusal(
            &format!("cannot run git: {e}"),
            "install git, on PATH",
        )]
    };
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(cannot)?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // Written beside the read, so neither pipe fills while the other waits;
    // a git that stops reading early says so by its exit.
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let out = child.wait_with_output().map_err(cannot)?;
    let _ = writer.join();
    match out.status.code() {
        Some(code) if ok.contains(&code) => Ok(out.stdout),
        _ => Err(vec![refusal(
            &format!(
                "`git {}` in {} failed: {}",
                args.join(" "),
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            "run it in a git repository, or give one with `--repo`",
        )]),
    }
}

/// The paths in git's `-z` output.
fn paths(out: &[u8]) -> Vec<String> {
    out.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// The top of the repo containing `dir`.
pub(crate) fn top(dir: &Path) -> Result<PathBuf, Vec<Refusal>> {
    let out = git(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&out).trim_end_matches('\n'),
    ))
}

/// Every path git tracks under `top`, from the top, in git's order.
pub(crate) fn tracked(top: &Path) -> Result<Vec<String>, Vec<Refusal>> {
    Ok(paths(&git(top, &["ls-files", "-z"])?))
}

/// The last `n` commits of HEAD's first-parent history, newest first, each
/// with the files it changed against its first parent; none before the
/// first commit.
pub(crate) fn history(top: &Path, n: usize) -> Result<Vec<Commit>, Vec<Refusal>> {
    if git(top, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_err() {
        return Ok(Vec::new());
    }
    let n = format!("-n{n}");
    let out = git(
        top,
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--first-parent",
            "-m",
            "--name-only",
            "--format=%x00%h %s",
            &n,
            "HEAD",
        ],
    )?;
    let out = String::from_utf8_lossy(&out);
    Ok(out
        .split('\0')
        .filter(|c| !c.is_empty())
        .map(|c| {
            let (header, files) = c.split_once('\n').unwrap_or((c, ""));
            let (commit, subject) = header.split_once(' ').unwrap_or((header, ""));
            Commit {
                commit: commit.to_string(),
                subject: subject.to_string(),
                files: files
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            }
        })
        .collect())
}

/// The bytes of each path a commit tracks.
pub(crate) type Sizes = BTreeMap<String, usize>;

/// The commit that last changed `path` (short), `file`'s text at it, and its
/// [`Sizes`]; none before a commit changes `path` or holds `file`.
pub(crate) fn at_last_change(
    top: &Path,
    path: &str,
    file: &str,
) -> Option<(String, String, Sizes)> {
    let commit = git(top, &["log", "-1", "--format=%h", "--", path]).ok()?;
    let commit = String::from_utf8_lossy(&commit).trim().to_string();
    let text = git(top, &["show", &format!("{commit}:{file}")]).ok()?;
    let tree = git(top, &["ls-tree", "-r", "-l", "-z", &commit]).ok()?;
    let sizes = tree.split(|b| *b == 0).filter_map(|e| {
        let e = String::from_utf8_lossy(e);
        let (header, path) = e.split_once('\t')?;
        Some((
            path.to_string(),
            header.split_whitespace().nth(3)?.parse().ok()?,
        ))
    });
    Some((
        commit,
        String::from_utf8_lossy(&text).into_owned(),
        sizes.collect(),
    ))
}

/// Those of `paths`, each from the top, that git's ignore rules match, with
/// no regard to whether git tracks them (`git check-ignore --no-index`):
/// one call for them all.
pub(crate) fn ignored(top: &Path, wanted: &BTreeSet<String>) -> Result<Vec<String>, Vec<Refusal>> {
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let input = wanted.iter().flat_map(|p| p.bytes().chain([0])).collect();
    // It exits 1 when it matches none of them.
    let args = ["check-ignore", "--no-index", "--stdin", "-z"];
    Ok(paths(&fed(top, &args, input, &[0, 1])?))
}
