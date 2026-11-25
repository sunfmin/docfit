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
    docfit(dir, "check")
}

fn docfit(dir: &Path, command: &str) -> (i32, Value) {
    let out = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir)
        .arg(command)
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
    assert_eq!(
        faults[1]["fix"],
        "add `\"notes.md\" = \"<kind>\"` under `[documents]` in docfit.toml, <kind> one of readme, agent, adr, glossary, design-note"
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
        (
            "[documents]\n\"README.md\" = { kind = \"readme\", size = 3 }\n",
            "give `README.md` one of the kinds readme, agent, adr, glossary, design-note, as `\"<kind>\"` or `{ kind",
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

#[test]
fn a_file_whose_name_says_its_kind_is_offered_that_kind() {
    let files = [
        ("docfit.toml", "[documents]\n"),
        ("a/README.md", ""),
        ("AGENTS.md", ""),
        ("GLOSSARY.md", ""),
    ];
    let (code, answer) = check(repo(&files).path());
    assert_eq!(code, 1, "{answer}");
    let kinds: Vec<_> = answer["faults"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["file"].clone(), f["kinds"].clone()))
        .collect();
    assert_eq!(
        serde_json::json!(kinds),
        serde_json::json!([
            ["AGENTS.md", ["agent"]],
            ["GLOSSARY.md", ["glossary"]],
            ["a/README.md", ["readme"]]
        ])
    );
}

#[test]
fn without_git_on_path_it_is_refused_with_its_fix() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", "r\n")]);
    let mut command = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir.path());
    let out = command
        .arg("check")
        .env("PATH", dir.path().join("no-bin"))
        .output()
        .unwrap();
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(2), "{answer}");
    assert_eq!(answer["error"][0]["fix"], "install git, on PATH");
}

const BIG: &str = "[documents]\n\"README.md\" = { kind = \"readme\", bytes = 10, line = 5 }\n";

fn baseline(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(".docfit/baseline.json")).unwrap())
        .unwrap()
}

#[test]
fn over_its_budget_with_no_baseline_is_red() {
    let dir = repo(&[("docfit.toml", BIG), ("README.md", "ab\nabcdefg\n")]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = answer["faults"].as_array().unwrap();
    assert_eq!(faults[0]["fault"], "over_budget");
    assert_eq!(
        (&faults[0]["size"], &faults[0]["limit"]),
        (&11.into(), &10.into())
    );
    assert_eq!(faults[1]["fault"], "line_too_long");
    assert_eq!(faults[1]["line"], 2);
    assert_eq!(
        (&faults[1]["size"], &faults[1]["limit"]),
        (&7.into(), &5.into())
    );
    let fix = faults[1]["fix"].as_str().unwrap();
    assert!(
        fix.starts_with("break line 2 of `README.md` at its sentences"),
        "{fix}"
    );
    assert!(fix.contains("`docfit fmt` (docfit#8)"), "{fix}");
    assert!(
        fix.contains("`docfit ratchet` writes .docfit/baseline.json"),
        "{fix}"
    );
}

#[test]
fn the_kinds_default_budget_holds_where_no_entry_sets_one() {
    let long = "x".repeat(201);
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", &long)]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    assert_eq!(answer["faults"][0]["fault"], "line_too_long");
    assert_eq!(answer["faults"][0]["limit"], 200);
    assert_eq!(answer["faults"].as_array().unwrap().len(), 1, "{answer}");
}

#[test]
fn over_budget_but_within_the_baseline_is_green_and_growth_past_it_red() {
    let dir = repo(&[("docfit.toml", BIG), ("README.md", "ab\nabcdefg\n")]);
    let (code, answer) = docfit(dir.path(), "ratchet");
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], true);
    let held = serde_json::json!({ "README.md": { "bytes": 11, "line": 7 } });
    assert_eq!(answer["baseline"], held);
    assert_eq!(baseline(dir.path()), held);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");

    std::fs::write(dir.path().join("README.md"), "ab\nabcdefgh\n").unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = answer["faults"].as_array().unwrap();
    assert_eq!(
        (&faults[0]["fault"], &faults[0]["limit"]),
        (&"over_budget".into(), &11.into())
    );
    assert!(
        faults[0]["fix"]
            .as_str()
            .unwrap()
            .contains("(it is 12, past the 11 .docfit/baseline.json holds it at)"),
        "{answer}"
    );
    assert_eq!(
        (&faults[1]["fault"], &faults[1]["limit"]),
        (&"line_too_long".into(), &7.into())
    );
    assert!(
        !faults[1]["fix"].as_str().unwrap().contains("adopting"),
        "{answer}"
    );
}

#[test]
fn ratchet_never_raises_a_number() {
    let toml = "[documents]\n\"*.md\" = { kind = \"readme\", bytes = 10, line = 5 }\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "abcdefghijkl\n"),
        ("A.md", "a\n"),
    ]);
    docfit(dir.path(), "ratchet");
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 13, "line": 12 } })
    );

    // Grown, and a new document over its budget: neither raises the baseline.
    std::fs::write(dir.path().join("README.md"), "abcdefghijklmn\n").unwrap();
    std::fs::write(dir.path().join("A.md"), "abcdefghijklmn\n").unwrap();
    let (code, answer) = docfit(dir.path(), "ratchet");
    assert_eq!((code, &answer["changed"]), (0, &false.into()), "{answer}");
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 13, "line": 12 } })
    );
    assert_eq!(check(dir.path()).0, 1);

    // Shrunk: lowered; its line back within budget leaves the baseline.
    std::fs::write(dir.path().join("README.md"), "abcd\nabcd\nab\n").unwrap();
    std::fs::write(dir.path().join("A.md"), "a\n").unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert!(
        answer["next"]
            .as_str()
            .unwrap()
            .starts_with("run `docfit ratchet`"),
        "{answer}"
    );
    let (_, answer) = docfit(dir.path(), "ratchet");
    assert_eq!(answer["changed"], true, "{answer}");
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 13 } })
    );
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert!(
        answer["next"]
            .as_str()
            .unwrap()
            .starts_with("nothing to do"),
        "{answer}"
    );

    // Within budget whole: the baseline is empty, and stays.
    std::fs::write(dir.path().join("README.md"), "ab\n").unwrap();
    docfit(dir.path(), "ratchet");
    assert_eq!(baseline(dir.path()), serde_json::json!({}));
    std::fs::write(dir.path().join("README.md"), "abcdefghijklmn\n").unwrap();
    docfit(dir.path(), "ratchet");
    assert_eq!(baseline(dir.path()), serde_json::json!({}));
}

#[test]
fn the_smallest_budget_of_the_matching_entries_holds() {
    let toml = "[documents]\n\"*.md\" = { kind = \"readme\", bytes = 10 }\n\"README.md\" = { kind = \"readme\", bytes = 3 }\n";
    let dir = repo(&[("docfit.toml", toml), ("README.md", "abcd\n")]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    assert_eq!(answer["faults"][0]["limit"], 3);
}

#[test]
fn a_baseline_or_document_it_cannot_read_is_refused() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", "r\n")]);
    std::fs::create_dir(dir.path().join(".docfit")).unwrap();
    std::fs::write(
        dir.path().join(".docfit/baseline.json"),
        "{\"README.md\": 3}",
    )
    .unwrap();
    for command in ["check", "ratchet"] {
        let (code, answer) = docfit(dir.path(), command);
        assert_eq!(code, 2, "{answer}");
        assert!(
            answer["error"][0]["fix"]
                .as_str()
                .unwrap()
                .starts_with("restore .docfit/baseline.json from git"),
            "{answer}"
        );
    }
    std::fs::remove_file(dir.path().join("README.md")).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"][0]["fix"],
        "restore `README.md` as UTF-8 text, or `git rm` it"
    );
}

#[test]
fn a_baseline_it_cannot_read_or_write_is_refused() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", "r\n")]);
    std::fs::write(dir.path().join(".docfit"), "").unwrap();
    let (code, answer) = docfit(dir.path(), "ratchet");
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["error"][0]["fix"]
            .as_str()
            .unwrap()
            .ends_with("writable, then run it again"),
        "{answer}"
    );
    std::fs::remove_file(dir.path().join(".docfit")).unwrap();
    std::fs::create_dir_all(dir.path().join(".docfit/baseline.json")).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"][0]["fix"],
        "make .docfit/baseline.json readable, or restore it from git"
    );
}
