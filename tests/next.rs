//! `docfit next` on a throwaway git repo: the baseline's debt as tasks, in the
//! shape slimfit's and truefit's `next --all` answer.

mod common;

use common::{docfit, isolated, repo};
use serde_json::json;
use std::process::Command;

/// A repo adopted with `SPEC.md` held to rehome and `README.md` over its budget.
fn indebted() -> tempfile::TempDir {
    let readme = format!(
        "# Readme\n\nThe answer is built by `Answer`.\n\n{}\n",
        "Words run on. ".repeat(400).trim_end()
    );
    let dir = repo(&[
        ("README.md", &readme),
        (
            "SPEC.md",
            "# Spec\n\nWe chose globs instead of regexes because they read.\n",
        ),
        ("src/a.rs", "struct Answer;\n"),
    ]);
    assert_eq!(docfit(dir.path(), &["init"]).0, 0);
    assert_eq!(docfit(dir.path(), &["fmt"]).0, 0);
    dir
}

#[test]
fn a_baseline_with_debt_answers_one_task_of_each_kind() {
    let dir = indebted();
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["by"],
        format!("docfit {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(answer["counts"], json!({ "rehome": 1, "over-budget": 1 }));
    let tasks = answer["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2, "{answer}");

    let rehome = &tasks[0];
    assert_eq!(rehome["kind"], "rehome");
    assert_eq!(rehome["files"], json!(["SPEC.md"]));
    assert_eq!(rehome["evidence"]["size"], rehome["evidence"]["held"]);
    assert_eq!(rehome["evidence"]["where"]["file"], "SPEC.md");
    assert_eq!(rehome["evidence"]["where"]["paragraphs"][0]["home"], "adr");
    let accept = rehome["accept"].as_str().unwrap();
    assert!(
        accept.contains("docfit ratchet") && accept.contains("`SPEC.md`"),
        "{accept}"
    );

    let over = &tasks[1];
    assert_eq!(over["kind"], "over-budget");
    assert_eq!(over["files"], json!(["README.md"]));
    assert_eq!(over["evidence"]["budget"]["bytes"], 4096);
    assert!(over["evidence"]["size"]["bytes"].as_u64().unwrap() > 4096);
    assert!(over["evidence"]["held"]["bytes"].is_u64(), "{over}");
    assert_eq!(over["evidence"]["where"]["kind"], "readme");
    assert_eq!(over["evidence"]["where"]["paragraphs"][0]["home"], "delete");

    assert_eq!(rehome["steps"], over["steps"]);
    let (_, first) = docfit(dir.path(), &["next"]);
    assert_eq!(first["task"], *rehome);
}

#[test]
fn the_steps_are_the_text_help_rehome_prints_and_put_the_map_in_the_commit() {
    let dir = indebted();
    let (_, answer) = docfit(dir.path(), &["next", "--all"]);
    let steps = answer["tasks"][0]["steps"].as_str().unwrap();
    let out = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir.path())
        .args(["help", "rehome"])
        .output()
        .unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    assert!(help.contains(steps), "{help}\n---\n{steps}");
    assert!(steps.contains("The commit that removes the content carries the map in its message"));
    assert!(steps.contains("never in a file of the repo"));
}

#[test]
fn a_task_paid_and_ratcheted_is_gone() {
    let dir = indebted();
    std::fs::remove_file(dir.path().join("SPEC.md")).unwrap();
    std::fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    assert_eq!(docfit(dir.path(), &["ratchet"]).0, 0);
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0);
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(answer["counts"], json!({ "rehome": 0, "over-budget": 0 }));
}

#[test]
fn a_repo_with_no_debt_answers_no_task() {
    let dir = repo(&[("README.md", "# Readme\n")]);
    let (_, before) = docfit(dir.path(), &["next", "--all"]);
    assert!(
        before["error"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("docfit init"),
        "{before}"
    );
    std::fs::write(
        dir.path().join("docfit.toml"),
        "[documents]\n\"README.md\" = \"readme\"\n",
    )
    .unwrap();
    let (code, before) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0);
    assert_eq!(before["tasks"], json!([]));
    assert!(
        before["notes"][0].as_str().unwrap().contains("docfit init"),
        "{before}"
    );
    assert_eq!(docfit(dir.path(), &["init"]).0, 0);
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0);
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(answer["notes"], json!([]));
    let (_, first) = docfit(dir.path(), &["next"]);
    assert_eq!(first["task"], json!(null));
}
