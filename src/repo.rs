//! The repo, read through git's own CLI: the only place that builds its
//! command lines.

use crate::cli::refusal;
use crate::documents::Refusal;
use crate::hot::Commit;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, Vec<Refusal>> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output();
    match out {
        Ok(out) if out.status.success() => Ok(out.stdout),
        Ok(out) => Err(vec![refusal(
            &format!(
                "`git {}` in {} failed: {}",
                args.join(" "),
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            "run it in a git repository, or give one with `--repo`",
        )]),
        Err(e) => Err(vec![refusal(
            &format!("cannot run git: {e}"),
            "install git, on PATH",
        )]),
    }
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
    let out = git(top, &["ls-files", "-z"])?;
    Ok(out
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
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
            let mut lines = c.lines();
            let (commit, subject) = lines
                .next()
                .unwrap_or("")
                .split_once(' ')
                .unwrap_or_default();
            Commit {
                commit: commit.to_string(),
                subject: subject.to_string(),
                files: lines
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            }
        })
        .collect())
}
