//! `docfit next` on a throwaway git repo: the baseline's debt as tasks, in the
//! shape slimfit's and truefit's `next --all` answer.

mod common;

use common::{docfit, isolated, repo};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

const CODE: &str = "fn main() {\n    let answer = check(repo);\n    print(answer);\n}\n";

/// A repo adopted with `SPEC.md` held to rehome, and `README.md` over its
/// budget and held at one local ref, one copied run and one measurement.
fn indebted() -> tempfile::TempDir {
    let readme = format!(
        "# Readme\n\nThe answer is built by `Answer`, as proj#7 asked.\n\nThe gate runs in 2 min.\n\n{CODE}\n{}\n",
        "Words run on. ".repeat(400).trim_end()
    );
    let dir = repo(&[
        ("docfit.toml", "[documents]\n\"README.md\" = \"readme\"\n"),
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
        json!({ "drop-local-issue-refs": 0, "rehome": 1, "over-budget": 1, "local-refs": 1, "restates": 1, "measurement": 1, "hot-file": 0, "must_read_bytes": 0 })
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
    let none = json!({ "drop-local-issue-refs": 0, "rehome": 0, "over-budget": 0, "local-refs": 0, "restates": 0, "measurement": 0, "hot-file": 0, "must_read_bytes": 0 });
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
    // Nothing committed yet: the total has nothing to be compared with.
    assert_eq!(
        answer["notes"],
        json!([
            "must_read_bytes 0; no commit has written .docfit/baseline.json beside a docfit.toml, so it is compared with nothing"
        ])
    );
    let (_, first) = docfit(dir.path(), &["next"]);
    assert_eq!(first["task"], json!(null));
}

/// A repo of 100 first-parent commits, newest last: `HOT.md` changed in 30
/// of them, `WARM.md` in 20, and `docfit.toml` written as `config`.
fn changed(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    let mut stream = String::new();
    for i in 0..100 {
        let msg = format!("change {i}");
        stream += &format!(
            "commit refs/heads/work\ncommitter A <a@b> {} +0000\ndata {}\n{msg}\n",
            1_700_000_000 + i,
            msg.len()
        );
        let mut files = vec![("src/n.txt", format!("{i}\n"))];
        if i == 0 {
            files.push(("docfit.toml", config.to_string()));
        }
        if i % 10 < 3 {
            files.push(("HOT.md", format!("# Hot\n\nChange {i}.\n")));
        }
        if i % 5 == 0 {
            files.push(("WARM.md", format!("# Warm\n\nChange {i}.\n")));
        }
        for (path, text) in files {
            stream += &format!("M 644 inline {path}\ndata {}\n{text}\n", text.len());
        }
    }
    let mut import = isolated(Command::new("git"), dir.path())
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(import.wait().unwrap().success());
    common::git(dir.path(), &["checkout", "-q", "-f", "work"]);
    dir
}

const REGISTERED: &str =
    "[documents]\n\"HOT.md\" = \"design-note\"\n\"WARM.md\" = \"design-note\"\n";

/// The files of each `hot-file` task `docfit next --all` answers in `dir`.
fn hot(dir: &std::path::Path) -> Vec<Value> {
    let (code, answer) = docfit(dir, &["next", "--all"]);
    assert_eq!(code, 0, "{answer}");
    let tasks = answer["tasks"].as_array().unwrap();
    let hot: Vec<Value> = tasks
        .iter()
        .filter(|t| t["kind"] == "hot-file")
        .cloned()
        .collect();
    assert_eq!(answer["counts"]["hot-file"], hot.len(), "{answer}");
    hot
}

#[test]
fn a_document_changed_in_more_than_a_quarter_of_the_last_100_commits_is_a_hot_file_task() {
    let dir = changed(REGISTERED);
    let hot = hot(dir.path());
    assert_eq!(hot.len(), 1, "{hot:?}");
    let task = &hot[0];
    assert_eq!(task["files"], json!(["HOT.md"]));
    let evidence = &task["evidence"];
    assert_eq!(
        (
            &evidence["share"],
            &evidence["changed"],
            &evidence["window"]
        ),
        (&json!(0.3), &json!(30), &json!(100))
    );
    let subjects: Vec<&str> = evidence["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["subject"].as_str().unwrap())
        .collect();
    let newest = [92, 91, 90, 82, 81, 80, 72, 71, 70, 62].map(|i| format!("change {i}"));
    assert_eq!(subjects, newest);
    let steps = task["steps"].as_str().unwrap();
    assert!(help(dir.path(), "hot-file").contains(steps), "{steps}");
    let accept = task["accept"].as_str().unwrap();
    assert!(
        accept.contains("`HOT.md` is within its kind's budget"),
        "{accept}"
    );
    // A signal, not a fault: the check is green on it.
    assert_eq!(docfit(dir.path(), &["check"]).0, 0);
}

#[test]
fn hot_in_docfit_toml_moves_the_line() {
    let lower = changed(&format!("hot = {{ share = 0.15 }}\n{REGISTERED}"));
    let files: Vec<Value> = hot(lower.path())
        .iter()
        .map(|t| t["files"][0].clone())
        .collect();
    assert_eq!(files, ["HOT.md", "WARM.md"]);
    // 3 of the last 10 change HOT.md, 2 WARM.md: neither past half.
    let window = changed(&format!(
        "hot = {{ window = 10, share = 0.5 }}\n{REGISTERED}"
    ));
    assert_eq!(hot(window.path()), Vec::<Value>::new());
    let bad = changed(&format!("hot = {{ share = 2 }}\n{REGISTERED}"));
    let (code, answer) = docfit(bad.path(), &["next", "--all"]);
    assert_eq!(code, 2);
    assert!(
        answer["error"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("share = 0.25"),
        "{answer}"
    );
}

#[test]
fn must_read_bytes_is_the_agent_docs_total_and_its_note_the_total_at_the_baselines_commit() {
    let (a, b) = ("# A\n\n".to_string() + &"a".repeat(95), "b".repeat(50));
    let dir = repo(&[
        (
            "docfit.toml",
            "[documents]\n\"docs/agents/*.md\" = \"agent\"\n\"README.md\" = \"readme\"\n",
        ),
        ("docs/agents/a.md", &a),
        ("docs/agents/b.md", &b),
        ("README.md", "# Readme\n"),
    ]);
    let commit = ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm"];
    common::git(dir.path(), &[&commit[..], &["first"]].concat());
    // A commit, but none with the baseline: compared with nothing.
    let (_, unwritten) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(unwritten["counts"]["must_read_bytes"], 150, "{unwritten}");
    assert!(
        unwritten["notes"][1]
            .as_str()
            .unwrap()
            .contains("compared with nothing"),
        "{unwritten}"
    );
    assert_eq!(docfit(dir.path(), &["init"]).0, 0);
    common::git(dir.path(), &["add", "-A"]);
    common::git(
        dir.path(),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "adopt",
        ],
    );
    let at = String::from_utf8(
        isolated(Command::new("git"), dir.path())
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    std::fs::write(dir.path().join("docs/agents/a.md"), a + "\nGrown.\n").unwrap();
    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["counts"]["must_read_bytes"], 158, "{answer}");
    assert_eq!(answer["tasks"], json!([]), "a trend, never a task");
    let want = format!("must_read_bytes 158, was 150 at {}", at.trim());
    assert!(
        answer["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().starts_with(&want)),
        "{answer}"
    );
    assert_eq!(docfit(dir.path(), &["check"]).0, 0);
}
