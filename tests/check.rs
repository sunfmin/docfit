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

#[cfg(unix)]
#[test]
fn a_baseline_it_cannot_read_or_write_is_refused() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", "r\n")]);
    use std::os::unix::fs::PermissionsExt;
    let held = dir.path().join(".docfit");
    std::fs::create_dir(&held).unwrap();
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (code, answer) = docfit(dir.path(), "ratchet");
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["error"][0]["fix"]
            .as_str()
            .unwrap()
            .ends_with("writable, then run it again"),
        "{answer}"
    );
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir(held.join("baseline.json")).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"][0]["fix"],
        "make .docfit/baseline.json readable, or restore it from git"
    );
}

#[test]
fn at_its_budget_exactly_is_within_it() {
    // Two lines of 7, the first reported; 16 bytes, not past a baseline.
    let toml = "[documents]\n\"README.md\" = { kind = \"readme\", bytes = 10, line = 6 }\n";
    let dir = repo(&[("docfit.toml", toml), ("README.md", "abcdefg\nabcdefg\n")]);
    let (_, answer) = check(dir.path());
    assert_eq!(answer["faults"][1]["line"], 1, "{answer}");
    assert!(
        answer["faults"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("(it is 16): "),
        "{answer}"
    );
    docfit(dir.path(), "ratchet");
    // Shrunk to its byte budget exactly: that measure leaves the baseline.
    std::fs::write(dir.path().join("README.md"), "abcdefg\nb\n").unwrap();
    docfit(dir.path(), "ratchet");
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "line": 7 } })
    );
}

#[test]
fn init_registers_each_file_by_its_place_and_holds_the_rest_to_rehome_green_that_day() {
    let big = "x\n".repeat(2500);
    let dir = repo(&[
        ("README.md", &big),
        ("CLAUDE.md", "c\n"),
        ("AGENTS.md", "a\n"),
        ("docs/agents/tracker.md", "t\n"),
        ("docs/adr/0001-one.md", "d\n"),
        ("GLOSSARY.md", "g\n"),
        ("SPEC.md", "spec\n"),
    ]);
    let (code, answer) = docfit(dir.path(), "init");
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["registered"],
        serde_json::json!({
            "AGENTS.md": "agent", "CLAUDE.md": "agent", "GLOSSARY.md": "glossary",
            "README.md": "readme", "docs/adr/0001-one.md": "adr", "docs/agents/tracker.md": "agent"
        })
    );
    let rehome = answer["rehome"].as_array().unwrap();
    assert_eq!(rehome.len(), 1, "{answer}");
    assert_eq!(rehome[0]["file"], "SPEC.md");
    assert_eq!(rehome[0]["size"], 5);
    assert_eq!(
        rehome[0]["kinds"],
        serde_json::json!(["readme", "agent", "adr", "glossary", "design-note"])
    );
    assert_eq!(answer["faults"], serde_json::json!([]), "{answer}");
    assert_eq!(answer["changed"], true);
    assert_eq!(
        answer["next"],
        "commit docfit.toml and .docfit/baseline.json, then `docfit check`; register a file in `rehome` under a kind where one fits it"
    );
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 5000 }, "SPEC.md": { "rehome": 5 } })
    );
    assert_eq!(answer["baseline"], baseline(dir.path()));
    let (code, checked) = check(dir.path());
    assert_eq!(code, 0, "{checked}");
    assert_eq!(checked["faults"], serde_json::json!([]), "{checked}");

    let toml = std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap();
    let (code, again) = docfit(dir.path(), "init");
    assert_eq!(code, 0, "{again}");
    assert_eq!(again["registered"], serde_json::json!({}), "{again}");
    assert_eq!(again["rehome"], serde_json::json!([]), "{again}");
    assert_eq!(again["changed"], false, "{again}");
    assert_eq!(again["next"], "nothing to adopt: run `docfit check`");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap(),
        toml
    );

    std::fs::write(dir.path().join("SPEC.md"), "spec!\n").unwrap();
    let (code, checked) = check(dir.path());
    assert_eq!(code, 1, "{checked}");
    assert_eq!(checked["faults"][0]["file"], "SPEC.md");
    assert_eq!(checked["faults"][0]["fault"], "rehome_grew");
    assert_eq!(checked["faults"][0]["size"], 6);
    assert_eq!(checked["faults"][0]["limit"], 5);
    assert_eq!(
        checked["faults"][0]["fix"],
        "cut `SPEC.md` back to at most 5 bytes (it is 6), the size .docfit/baseline.json holds it at to rehome: move its content to the documents whose kinds it holds, or register it under a kind"
    );

    std::fs::write(dir.path().join("notes.md"), "n\n").unwrap();
    std::fs::write(dir.path().join("SPEC.md"), "sp\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    let (code, checked) = check(dir.path());
    assert_eq!(code, 1, "{checked}");
    assert_eq!(checked["faults"].as_array().unwrap().len(), 1, "{checked}");
    assert_eq!(checked["faults"][0]["fault"], "unregistered");
    assert_eq!(checked["faults"][0]["file"], "notes.md");
    docfit(dir.path(), "ratchet");
    assert_eq!(
        baseline(dir.path())["SPEC.md"],
        serde_json::json!({ "rehome": 3 })
    );
    assert_eq!(baseline(dir.path()).get("notes.md"), None);

    git(dir.path(), &["rm", "-qf", "SPEC.md", "notes.md"]);
    let (code, checked) = check(dir.path());
    assert_eq!(code, 0, "{checked}");
    assert!(
        checked["next"]
            .as_str()
            .unwrap()
            .starts_with("run `docfit ratchet`"),
        "{checked}"
    );
    let (_, ratcheted) = docfit(dir.path(), "ratchet");
    assert_eq!(ratcheted["changed"], true);
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 5000 } })
    );
}

#[test]
fn init_keeps_what_is_registered_and_adds_only_what_is_not() {
    let toml = "# ours\n[documents]\n\"README.md\" = { kind = \"readme\", bytes = 100 }\n\n[other]\nx = 1\n";
    let big = "y\n".repeat(1500);
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "r\n"),
        ("CLAUDE.md", &big),
    ]);
    let (code, answer) = docfit(dir.path(), "init");
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["registered"],
        serde_json::json!({ "CLAUDE.md": "agent" })
    );
    assert_eq!(answer["rehome"], serde_json::json!([]));
    assert_eq!(answer["changed"], true);
    let written = std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap();
    assert_eq!(
        written,
        "# ours\n[documents]\n\"README.md\" = { kind = \"readme\", bytes = 100 }\n\"CLAUDE.md\" = \"agent\"\n\n[other]\nx = 1\n"
    );
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "CLAUDE.md": { "bytes": 3000 } })
    );
    let (code, checked) = check(dir.path());
    assert_eq!(code, 0, "{checked}");
    assert_eq!(checked["documents"]["CLAUDE.md"], "agent");
}

#[test]
fn init_refuses_a_docfit_toml_it_cannot_read_and_writes_nothing() {
    let dir = repo(&[("docfit.toml", "[documents\n"), ("README.md", "r\n")]);
    let (code, answer) = docfit(dir.path(), "init");
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["error"][0]["fault"]
            .as_str()
            .unwrap()
            .contains("not TOML")
    );
    assert!(!dir.path().join(".docfit").exists());
}

#[test]
fn init_refuses_a_documents_table_it_cannot_add_a_line_to() {
    let toml = "documents = { \"README.md\" = \"readme\" }\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "r\n"),
        ("CLAUDE.md", "c\n"),
    ]);
    let (code, answer) = docfit(dir.path(), "init");
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"][0]["fix"],
        "write `documents` in docfit.toml as a `[documents]` table, a line per glob"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap(),
        toml
    );
}

#[test]
fn a_tracked_file_gone_from_the_working_tree_is_refused_with_its_fix() {
    let dir = repo(&[
        ("docfit.toml", README_ONLY),
        ("README.md", "r\n"),
        ("SPEC.md", "s\n"),
    ]);
    std::fs::remove_file(dir.path().join("SPEC.md")).unwrap();
    for command in ["check", "ratchet", "init"] {
        let (code, answer) = docfit(dir.path(), command);
        assert_eq!(code, 2, "{command}: {answer}");
        assert_eq!(
            answer["error"][0]["fix"],
            "restore `SPEC.md`, or `git rm` it"
        );
    }
}

#[test]
fn init_on_a_repo_with_a_baseline_leaves_it_and_answers_what_check_would() {
    let dir = repo(&[
        ("docfit.toml", "[other]\nx = 1"),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("notes.md", "n\n"),
    ]);
    let (code, answer) = docfit(dir.path(), "init");
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["registered"],
        serde_json::json!({ "README.md": "readme" })
    );
    assert_eq!(answer["rehome"], serde_json::json!([]));
    assert_eq!(answer["faults"][0]["fault"], "unregistered", "{answer}");
    assert_eq!(answer["faults"][0]["file"], "notes.md");
    assert_eq!(answer["next"], answer["faults"][0]["fix"]);
    assert_eq!(answer["changed"], true);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap(),
        "[other]\nx = 1\n\n[documents]\n\"README.md\" = \"readme\"\n"
    );
    assert_eq!(baseline(dir.path()), serde_json::json!({}));
}

#[test]
fn a_docfit_toml_it_cannot_read_is_refused_with_its_fix() {
    let dir = repo(&[("README.md", "r\n")]);
    std::fs::create_dir(dir.path().join("docfit.toml")).unwrap();
    for command in ["check", "init"] {
        let (code, answer) = docfit(dir.path(), command);
        assert_eq!(code, 2, "{command}: {answer}");
        assert_eq!(
            answer["error"][0]["fix"],
            "make docfit.toml readable UTF-8 text, or restore it from git"
        );
    }
}
