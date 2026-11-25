//! Throwaway git repos and the real binary run in them: shared by the tests.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// A git repo in a temp dir, tracking `files` (path, text).
pub fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    dir
}

pub fn git(dir: &Path, args: &[&str]) {
    let ok = isolated(Command::new("git"), dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Kept from the user's git config and from any repo enclosing the temp dir.
pub fn isolated(mut command: Command, dir: &Path) -> Command {
    command
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap());
    command
}

/// docfit run with `args` in `dir`: its exit code and its JSON answer.
pub fn docfit(dir: &Path, args: &[&str]) -> (i32, Value) {
    let out = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir)
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}")),
    )
}
