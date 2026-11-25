//! The real binary: what it prints where, and its exit code.

use std::process::Command;

fn docfit(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_docfit"))
        .args(args)
        .output()
        .expect("docfit runs")
}

#[test]
fn help_goes_to_stdout_with_exit_0() {
    let out = docfit(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("docfit: "));
    assert!(out.stderr.is_empty());
}

#[test]
fn a_refusal_goes_to_stderr_with_exit_2() {
    let out = docfit(&["nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("\"fix\":\"run `docfit --help`\""));
}
