//! `docfit check` on throwaway git repos: its JSON and exit code.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// A git repo in a temp dir, tracking `files` (path, text).
fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
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

fn git(dir: &Path, args: &[&str]) {
    let ok = isolated(Command::new("git"), dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Kept from the user's git config and from any repo enclosing the temp dir.
fn isolated(mut command: Command, dir: &Path) -> Command {
    command
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap());
    command
}

fn check(dir: &Path) -> (i32, Value) {
    let out = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir)
        .arg("check")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}")),
    )
}

const README_ONLY: &str = "[documents]\n\"README.md\" = \"readme\"\n";

#[test]
fn an_unregistered_markdown_file_is_red_with_the_line_that_registers_it() {
    let dir = repo(&[
        ("docfit.toml", README_ONLY),
        ("README.md", "# r\n"),
        ("docs/agents/tracker.md", "x\n"),
        ("notes.md", "x\n"),
        ("docs/adr/0001-one.md", "x\n"),
        ("src/main.rs", "fn main() {}\n"),
    ]);
    std::fs::write(dir.path().join("untracked.md"), "x\n").unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = answer["faults"].as_array().unwrap();
    assert_eq!(faults.len(), 3, "{answer}");
    assert_eq!(faults[0]["file"], "docs/adr/0001-one.md");
    assert_eq!(faults[0]["kinds"], serde_json::json!(["adr"]));
    let faults = &faults[1..];
    assert_eq!(faults[0]["file"], "docs/agents/tracker.md");
    assert_eq!(faults[0]["fault"], "unregistered");
    assert_eq!(faults[0]["kinds"], serde_json::json!(["agent"]));
    assert_eq!(
        faults[0]["fix"],
        "add `\"docs/agents/tracker.md\" = \"agent\"` under `[documents]` in docfit.toml"
    );
    assert_eq!(faults[1]["file"], "notes.md");
    assert_eq!(
        faults[1]["kinds"],
        serde_json::json!(["readme", "agent", "adr", "glossary", "design-note"])
    );
    assert!(
        faults[1]["fix"]
            .as_str()
            .unwrap()
            .contains("`\"notes.md\" = \"<kind>\"`"),
        "{answer}"
    );
    assert_eq!(answer["next"], answer["faults"][0]["fix"]);
}

#[test]
fn every_markdown_file_registered_under_a_kind_is_green() {
    let toml = "[documents]\n\"README.md\" = \"readme\"\n\"docs/**/*.md\" = \"agent\"\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "r\n"),
        ("docs/agents/a.md", "a\n"),
        ("docs/b.md", "b\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["faults"], serde_json::json!([]));
    assert_eq!(answer["documents"]["docs/agents/a.md"], "agent");
    assert_eq!(answer["documents"]["README.md"], "readme");
    assert!(answer["next"].is_string());
}

#[test]
fn a_file_registered_under_two_kinds_is_red() {
    let toml = "[documents]\n\"*.md\" = \"readme\"\n\"GLOSSARY.md\" = \"glossary\"\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "r\n"),
        ("GLOSSARY.md", "g\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    assert_eq!(answer["faults"][0]["file"], "GLOSSARY.md");
    assert_eq!(answer["faults"][0]["fault"], "two_kinds");
    assert_eq!(
        answer["faults"][0]["kinds"],
        serde_json::json!(["readme", "glossary"])
    );
}

#[test]
fn an_unknown_kind_is_refused_with_the_kinds_listed() {
    let toml =
        "[documents]\n\"README.md\" = \"readme\"\n\"SPEC.md\" = \"spec\"\n\"x/*.md\" = \"notes\"\n";
    let dir = repo(&[("docfit.toml", toml), ("README.md", "r\n")]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    let errors = answer["error"].as_array().unwrap();
    assert_eq!(errors.len(), 2, "{answer}");
    assert_eq!(
        errors[0]["fault"],
        "`SPEC.md` names the kind `spec`, which docfit does not know"
    );
    assert_eq!(
        errors[0]["fix"],
        "give `SPEC.md` one of the kinds readme, agent, adr, glossary, design-note"
    );
}

#[test]
fn a_repo_without_docfit_toml_is_refused_with_how_to_start_one() {
    let dir = repo(&[("README.md", "r\n")]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["error"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("[documents]"),
        "{answer}"
    );
}

#[test]
fn a_docfit_toml_docfit_cannot_read_is_refused_with_its_fix() {
    for (toml, fix) in [
        ("[documents\n", "fix docfit.toml"),
        ("[other]\n", "add a `[documents]` table to docfit.toml"),
        (
            "[documents]\n\"docs/[a.md\" = \"agent\"\n",
            "fix `docs/[a.md` in docfit.toml",
        ),
        (
            "[documents]\n\"README.md\" = 1\n",
            "give `README.md` one of the kinds",
        ),
    ] {
        let dir = repo(&[("docfit.toml", toml), ("README.md", "r\n")]);
        let (code, answer) = check(dir.path());
        assert_eq!(code, 2, "{toml}: {answer}");
        assert!(
            answer["error"][0]["fix"].as_str().unwrap().starts_with(fix),
            "{toml}: {answer}"
        );
    }
}

#[test]
fn outside_a_git_repository_it_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"][0]["fix"],
        "run it in a git repository, or give one with `--repo`"
    );
}
