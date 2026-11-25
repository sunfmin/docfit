//! The real binary: its stdout and exit code; it writes nothing to stderr.

use std::process::Command;

fn docfit(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_docfit"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

#[test]
fn help_is_the_manual() {
    let (code, out) = docfit(&["--help"]);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("# `docfit`\n\nKeeps a repo's documents each in its one home"),
        "{out}"
    );
    assert!(
        out.contains(
            "| kind | reader | holds only |\n|---|---|---|\n| `readme` | someone new to the repo |"
        ),
        "{out}"
    );
    assert!(
        out.contains("| `design-note` | whoever changes one cross-module mechanism |"),
        "{out}"
    );
}

#[test]
fn an_unknown_command_is_refused_with_its_fix() {
    let (code, out) = docfit(&["nope"]);
    assert_eq!(code, 2);
    let answer: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(answer["error"][0]["fix"], "run `docfit --help`");
}

#[test]
fn version_names_the_crate_version() {
    assert_eq!(
        docfit(&["--version"]),
        (0, format!("docfit {}\n", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn an_unknown_topic_is_refused_with_its_fix() {
    let (code, out) = docfit(&["help", "nope"]);
    assert_eq!(code, 2);
    let answer: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        answer["error"][0]["fix"],
        "run `docfit --help` for the topics"
    );
}

#[test]
fn help_answers_the_index_and_help_all_the_whole_manual() {
    let index = docfit(&["--help"]);
    assert_eq!(docfit(&["help"]), index);
    let (code, all) = docfit(&["help", "all"]);
    assert_eq!(code, 0);
    assert!(all.contains("## When to run it"), "{all}");
    assert!(!index.1.contains("## When to run it"), "{}", index.1);
}
