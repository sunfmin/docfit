//! `docfit next` on a throwaway git repo: the baseline's debt as tasks, in the
//! shape slimfit's and truefit's `next --all` answer.

mod common;

use common::{docfit, isolated, repo};
use serde_json::{Value, json};
use std::process::Command;

const CODE: &str = "fn main() {\n    let answer = check(repo);\n    print(answer);\n}\n";

/// A repo adopted with `SPEC.md` held to rehome, and `README.md` over its
/// budget and held at one local ref, one copied run and one measurement.
fn indebted() -> tempfile::TempDir {
    let readme = format!(
        "# Readme\n\nThe answer is built by `Answer`, as proj#7 asked.\n\nThe gate runs in 2 min.\n\n{CODE}\n{}\n",
        "Words run on. ".repeat(400).trim_end()
    );
    let dir = repo(&[
        (
            "docfit.toml",
            "local_issue_refs = [\"proj\"]\n[documents]\n\"README.md\" = \"readme\"\n",
        ),
        ("README.md", &readme),
        (
            "SPEC.md",
            "# Spec\n\nWe chose globs instead of regexes because they read.\n",
        ),
        ("src/a.rs", &format!("struct Answer;\n{CODE}")),
    ]);
    assert_eq!(docfit(dir.path(), &["init"]).0, 0);
    dir
}

#[test]
fn a_baseline_with_debt_answers_one_task_of_each_kind() {
    let dir = indebted();
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["by"], format!("docfit {}", env!("DOCFIT_VERSION")));
    assert_eq!(
        answer["counts"],
        json!({ "rehome": 1, "over-budget": 1, "local-refs": 1, "restates": 1, "measurement": 1 })
    );
    let tasks = answer["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 5, "{answer}");

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
    for (task, kind) in tasks[2..]
        .iter()
        .zip(["local-refs", "restates", "measurement"])
    {
        assert_eq!(task["kind"], kind);
        assert_eq!(task["files"], json!(["README.md"]));
        let steps = task["steps"].as_str().unwrap();
        assert!(help(dir.path(), kind).contains(steps), "{kind}: {steps}");
        let accept = task["accept"].as_str().unwrap();
        assert!(
            accept.contains("docfit ratchet") && accept.contains("`README.md`"),
            "{accept}"
        );
        assert!(task["evidence"]["where"].is_null(), "{task}");
    }
    assert_eq!(
        tasks[2]["evidence"],
        json!({ "refs": [{ "line": 3, "ref": "proj#7" }], "held": 1 })
    );
    assert_eq!(
        tasks[3]["evidence"],
        json!({
            "strings": [],
            "copies": [{ "line": 7, "end": 10, "found": "fn main() {", "from": "src/a.rs:2" }],
            "held": { "strings": 0, "copies": 1 },
        })
    );
    assert_eq!(
        tasks[4]["evidence"],
        json!({ "measurements": [{ "line": 5, "found": "2 min" }], "held": 1 })
    );
    let steps = tasks[2]["steps"].as_str().unwrap();
    assert!(steps.contains("factorium issue show"), "{steps}");
    let (_, first) = docfit(dir.path(), &["next"]);
    assert_eq!(first["task"], *rehome);
}

/// What `docfit help <topic>` prints.
fn help(dir: &std::path::Path, topic: &str) -> String {
    let out = isolated(Command::new(env!("CARGO_BIN_EXE_docfit")), dir)
        .args(["help", topic])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_steps_are_the_text_help_rehome_prints_and_put_the_map_in_the_commit() {
    let dir = indebted();
    let (_, answer) = docfit(dir.path(), &["next", "--all"]);
    let steps = answer["tasks"][0]["steps"].as_str().unwrap();
    let help = help(dir.path(), "rehome");
    assert!(help.contains(steps), "{help}\n---\n{steps}");
    assert!(steps.contains("The commit that removes the content carries the map in its message"));
    assert!(steps.contains("never in a file of the repo"));
}

#[test]
fn a_task_paid_and_ratcheted_is_gone() {
    let none =
        json!({ "rehome": 0, "over-budget": 0, "local-refs": 0, "restates": 0, "measurement": 0 });
    let dir = indebted();
    std::fs::remove_file(dir.path().join("SPEC.md")).unwrap();
    std::fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    // Held, but paid: no task even before `docfit ratchet` drops it.
    assert_eq!(docfit(dir.path(), &["next", "--all"]).1["counts"], none);
    assert_eq!(docfit(dir.path(), &["ratchet"]).0, 0);
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0);
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(answer["counts"], none);
    // Found anew, but not held: `docfit check` is red on it, and it is no task.
    let readme = format!("# Readme\n\nIt runs in 2 min, as proj#8 asked.\n\n{CODE}");
    std::fs::write(dir.path().join("README.md"), readme).unwrap();
    let (code, check) = docfit(dir.path(), &["check"]);
    assert_eq!(code, 1);
    let faults: Vec<&Value> = check["faults"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["fault"])
        .collect();
    assert_eq!(faults, ["local_ref", "restates", "measurement"], "{check}");
    assert_eq!(docfit(dir.path(), &["next", "--all"]).1["counts"], none);
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
