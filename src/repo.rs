//! The repo, read through git's own CLI: the only place that builds its
//! command lines.

use crate::cli::refusal;
use crate::documents::Refusal;
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
