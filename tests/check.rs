//! `docfit check`, `ratchet` and `fmt` on throwaway git repos: their JSON and exit code.

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
    docfit(dir, &["check"])
}

fn docfit(dir: &Path, args: &[&str]) -> (i32, Value) {
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
    assert!(fix.contains("`docfit fmt README.md` breaks it"), "{fix}");
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
    let (code, answer) = docfit(dir.path(), &["ratchet"]);
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
    docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "bytes": 13, "line": 12 } })
    );

    // Grown, and a new document over its budget: neither raises the baseline.
    std::fs::write(dir.path().join("README.md"), "abcdefghijklmn\n").unwrap();
    std::fs::write(dir.path().join("A.md"), "abcdefghijklmn\n").unwrap();
    let (code, answer) = docfit(dir.path(), &["ratchet"]);
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
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
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
    docfit(dir.path(), &["ratchet"]);
    assert_eq!(baseline(dir.path()), serde_json::json!({}));
    std::fs::write(dir.path().join("README.md"), "abcdefghijklmn\n").unwrap();
    docfit(dir.path(), &["ratchet"]);
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
        let (code, answer) = docfit(dir.path(), &[command]);
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
    let (code, answer) = docfit(dir.path(), &["ratchet"]);
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
    docfit(dir.path(), &["ratchet"]);
    // Shrunk to its byte budget exactly: that measure leaves the baseline.
    std::fs::write(dir.path().join("README.md"), "abcdefg\nb\n").unwrap();
    docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        baseline(dir.path()),
        serde_json::json!({ "README.md": { "line": 7 } })
    );
}

const POINTED: &str = "[documents]\n\"README.md\" = \"readme\"\n\"docs/**/*.md\" = \"agent\"\n";

/// The `broken_pointer` faults of `readme` (and `docs/agents/a.md`, `guide`),
/// each as (file, line, "<pointer> -> <missing>").
fn broken(readme: &str, guide: &str) -> Vec<(String, u64, String)> {
    let dir = repo(&[
        ("docfit.toml", POINTED),
        ("README.md", readme),
        ("docs/agents/a.md", guide),
        ("docs/adr/0001-one.md", "x\n"),
        ("src/main.rs", "fn main() {}\n"),
    ]);
    let (code, answer) = check(dir.path());
    let faults: Vec<_> = answer["faults"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["fault"] == "broken_pointer")
        .map(|f| {
            let fix = f["fix"].as_str().unwrap();
            assert!(fix.contains(f["missing"].as_str().unwrap()), "{fix}");
            (
                f["file"].as_str().unwrap().to_string(),
                f["line"].as_u64().unwrap(),
                format!(
                    "{} -> {}",
                    f["pointer"].as_str().unwrap(),
                    f["missing"].as_str().unwrap()
                ),
            )
        })
        .collect();
    assert_eq!(code, if faults.is_empty() { 0 } else { 1 }, "{answer}");
    faults
}

fn at(file: &str, line: u64, missing: &str) -> (String, u64, String) {
    (file.to_string(), line, missing.to_string())
}

#[test]
fn a_backticked_path_that_exists_is_green_and_one_that_does_not_red() {
    let readme = "Run `src/main.rs:1`, see `docs/agents/`, `docfit.toml` and `docs/adr`.\n\
        Not paths: `docfit --help`, `*.md`, `<dir>/x`, `~/a/b`, `/usr/bin`, `.md`, `Kind::ALL`, `a.b`, `nowhere.md:x`.\n\
        Gone: `src/gone.rs` and `notes.toml`.\n";
    assert_eq!(
        broken(
            readme,
            "Here `a.md`, from the top `README.md`; not `b.md`.\n"
        ),
        [
            at("README.md", 3, "src/gone.rs -> src/gone.rs"),
            at("README.md", 3, "notes.toml -> notes.toml"),
            at("docs/agents/a.md", 1, "b.md -> b.md"),
        ]
    );
}

#[test]
fn a_link_to_a_missing_file_is_red_and_a_url_is_not_checked() {
    let readme = "[a](docs/agents/a.md#top), [`src`](src/), [t](</src/main.rs> \"title\")\n\
        [web](https://example.com/x.md), [mail](mailto:a@b.c), [anchor](#here)\n\
        ```\n[in a fence](nowhere.md) `nowhere.md`\n```\n\
        [gone](docs/gone.md)\n";
    let guide =
        "[up](../../README.md), [adr](../adr/0001-one.md), [out](../../../x.md), [no](c.md)\n";
    assert_eq!(
        broken(readme, guide),
        [
            at("README.md", 6, "docs/gone.md -> docs/gone.md"),
            at("docs/agents/a.md", 1, "../../../x.md -> ../../../x.md"),
            at("docs/agents/a.md", 1, "c.md -> docs/agents/c.md"),
        ]
    );
}

#[test]
fn an_adr_named_by_a_number_with_no_file_of_it_is_red() {
    let readme =
        "Per ADR 0001 and ADR-1, ADRs, `ADR 0002`, BADR 0003, ADR 0004x, ADR-.\nBut ADR 0009.\n";
    assert_eq!(
        broken(readme, "Or ADR-0010.\n"),
        [
            at("README.md", 2, "ADR 0009 -> docs/adr/0009-*"),
            at("docs/agents/a.md", 1, "ADR-0010 -> docs/adr/0010-*"),
        ]
    );
}

const PROSE: &str = "# Title. Still the title.\n\nOne. Two? Three!\n- 第一句。第二句。\n\n```sh\necho a. b.\n```\n\n| a. b. | c. d. |\n|---|---|\n";

#[test]
fn an_unformatted_document_is_red_until_fmt_breaks_it_at_its_sentences() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", PROSE)]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    assert_eq!(
        answer["faults"],
        serde_json::json!([{
            "file": "README.md", "fault": "unformatted", "line": 3,
            "fix": "run `docfit fmt README.md`: it breaks its prose at its sentences, one per line",
        }])
    );
    let (code, answer) = docfit(dir.path(), &["fmt", "README.md"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], true);
    assert_eq!(answer["formatted"], serde_json::json!(["README.md"]));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "# Title. Still the title.\n\nOne.\nTwo?\nThree!\n- 第一句。\n  第二句。\n\n```sh\necho a. b.\n```\n\n| a. b. | c. d. |\n|---|---|\n"
    );
    assert!(!dir.path().join("README.md.tmp").exists());
    let (code, answer) = check(dir.path());
    assert_eq!(
        (code, &answer["faults"]),
        (0, &serde_json::json!([])),
        "{answer}"
    );
}

#[test]
fn a_second_fmt_changes_nothing() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", PROSE)]);
    docfit(dir.path(), &["fmt"]);
    let once = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
    let (code, answer) = docfit(dir.path(), &["fmt"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], false);
    assert_eq!(answer["formatted"], serde_json::json!([]));
    assert_eq!(answer["next"], "nothing to format: run `docfit check`");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        once
    );
}

#[test]
fn fmt_leaves_a_file_that_is_no_registered_document_alone() {
    let dir = repo(&[
        ("docfit.toml", README_ONLY),
        ("README.md", "A. B.\n"),
        ("notes.md", "C. D.\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["fmt"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["formatted"], serde_json::json!(["README.md"]));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.md")).unwrap(),
        "C. D.\n"
    );
}

#[test]
fn fmt_naming_a_file_that_is_no_registered_document_is_refused_and_writes_nothing() {
    let dir = repo(&[
        ("docfit.toml", README_ONLY),
        ("README.md", "A. B.\n"),
        ("notes.md", "C. D.\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["fmt", "README.md", "notes.md"]);
    assert_eq!(code, 2, "{answer}");
    assert_eq!(
        answer["error"],
        serde_json::json!([{
            "fault": "`notes.md` is not a registered document",
            "fix": "name it from the repo's top as `docfit check` lists it under `documents`, or register it in docfit.toml",
        }])
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "A. B.\n"
    );
}

#[test]
fn fmt_on_a_document_it_cannot_write_is_refused() {
    let dir = repo(&[("docfit.toml", README_ONLY), ("README.md", "A. B.\n")]);
    std::fs::create_dir(dir.path().join("README.md.tmp")).unwrap();
    let (code, answer) = docfit(dir.path(), &["fmt"]);
    assert_eq!(code, 2, "{answer}");
    let fix = answer["error"][0]["fix"].as_str().unwrap();
    assert!(
        fix.ends_with("README.md writable, then run it again"),
        "{fix}"
    );
}
