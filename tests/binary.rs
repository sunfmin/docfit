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
            "| kind | reader | holds only | bytes | longest paragraph |\n|---|---|---|---|---|\n| `readme` | someone new to the repo |"
        ),
        "{out}"
    );
    assert!(
        out.contains("| `design-note` | whoever changes one cross-module mechanism |"),
        "{out}"
    );
    assert!(
        out.contains("one mechanism; off unless the repo turns it on | 6144 | 600 |"),
        "{out}"
    );
    assert!(out.contains("## Budgets"), "{out}");
    let (_, check) = docfit(&["check", "--help"]);
    assert!(check.contains("one of `.md`, `.toml`, `.json`,"), "{check}");
    for (holds, bytes) in [
        ("how to build and run it", 4096),
        ("unwritten conventions", 2048),
        ("one decision and why", 3072),
        ("a line or two each", 8192),
        ("how to do one task", 6144),
    ] {
        assert!(
            out.contains(&format!("{holds} | {bytes} | 600 |")),
            "{holds}: {out}"
        );
    }
    assert!(
        out.contains("| what the product shows or publishes | none; an entry may set one | 600 |"),
        "{out}"
    );
    assert!(
        check.contains("Every repo ignores `.slimfit/**`, `.truefit/**`, the fits' own records"),
        "{check}"
    );
    let (_, init) = docfit(&["init", "--help"]);
    assert!(
        init.contains("with one of `src`, `tests`, `test`, `fixtures`, `testdata`, `__fixtures__` as a directory"),
        "{init}"
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
        (0, format!("docfit {}\n", env!("DOCFIT_VERSION")))
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
    assert!(!all.to_lowercase().contains("sentence"), "{all}");
}

#[test]
fn fmt_is_an_unknown_command() {
    let (code, out) = docfit(&["fmt"]);
    assert_eq!(code, 2, "{out}");
    let answer: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        answer["error"][0]["fault"]
            .as_str()
            .unwrap()
            .contains("unrecognized subcommand 'fmt'"),
        "{answer}"
    );
}
