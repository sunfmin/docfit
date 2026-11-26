//! `docfit check`, `ratchet`, `init` and `next` on local tracker refs: by the
//! rule, which needs no list, and by docfit 0.1.3's `local_issue_refs`, with
//! the upgrade from one to the other: their JSON and exit code.

mod common;

use common::{docfit, repo};
use serde_json::{Value, json};
use std::path::Path;

const ON: &str =
    "local_issue_refs = [\"rollmind\", \"lore\"]\n[documents]\n\"README.md\" = \"readme\"\n";

fn check(dir: &Path) -> (i32, Value) {
    docfit(dir, &["check"])
}

#[test]
fn a_new_ref_in_a_code_comment_is_red_naming_its_line_and_the_ref() {
    let dir = repo(&[
        ("docfit.toml", ON),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        (
            "src/a.rs",
            "fn a() {}\n\n// fixes rollmind#38, see lore#7\n",
        ),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let fault = &answer["faults"][0];
    assert_eq!(fault["file"], "src/a.rs");
    assert_eq!(fault["fault"], "local_ref");
    assert_eq!(
        (&fault["line"], &fault["ref"]),
        (&json!(3), &json!("rollmind#38"))
    );
    assert_eq!((&fault["size"], &fault["limit"]), (&json!(2), &json!(0)));
    let fix = fault["fix"].as_str().unwrap();
    assert!(
        fix.contains("replace it with what it points at: the person's words with who, date and where; or the command and the number it printed; or delete it"),
        "{fix}"
    );
    assert_eq!(answer["next"], fault["fix"]);
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        (&answer["changed"], &answer["baseline"]),
        (&json!(false), &json!({}))
    );
}

#[test]
fn a_ref_with_an_owner_or_in_a_url_is_no_ref() {
    let text = "sunfmin/lore#25\nhttps://github.com/sunfmin/rollmind/issues/3\n";
    let dir = repo(&[
        ("docfit.toml", ON),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", text),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn the_fits_records_and_what_ignore_matches_are_not_read() {
    let toml = format!("ignore = [\"vendor/**\"]\n{ON}");
    let dir = repo(&[
        ("docfit.toml", &toml),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        (".slimfit/growth/x.json", "{\"serves\": \"rollmind#84\"}\n"),
        (".truefit/x.json", "{\"serves\": \"rollmind#84\"}\n"),
        ("vendor/a.c", "// rollmind#1\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn a_file_held_at_4_and_cut_to_3_is_green_and_ratchet_lowers_it_then_drops_it_at_0() {
    let refs = |n: usize| "// rollmind#1\n".repeat(n);
    let dir = repo(&[
        ("docfit.toml", ON),
        (
            ".docfit/baseline.json",
            "{\"src/a.rs\": {\"local_refs\": 4}}\n",
        ),
        ("README.md", "r\n"),
        ("src/a.rs", &refs(3)),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert!(
        answer["next"].as_str().unwrap().contains("docfit ratchet"),
        "{answer}"
    );
    let (code, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["baseline"], json!({"src/a.rs": {"local_refs": 3}}));

    std::fs::write(dir.path().join("src/a.rs"), refs(4)).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    assert_eq!(
        (&answer["faults"][0]["line"], &answer["faults"][0]["limit"]),
        (&json!(4), &json!(3))
    );

    std::fs::write(dir.path().join("src/a.rs"), "").unwrap();
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(answer["baseline"], json!({}));
}

#[test]
fn a_hold_beside_a_budgets_and_a_rehome_is_kept_and_lowered() {
    let toml = "local_issue_refs = [\"rollmind\"]\n[documents]\n\"README.md\" = { kind = \"readme\", bytes = 10 }\n";
    let held = "{\"README.md\": {\"bytes\": 40, \"local_refs\": 2}, \"SPEC.md\": {\"rehome\": 40, \"local_refs\": 2}}\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        (".docfit/baseline.json", held),
        ("README.md", "rollmind#1 rollmind#2 xxxxxxxxxx\n"),
        ("SPEC.md", "rollmind#1\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        answer["baseline"],
        json!({"README.md": {"bytes": 33, "local_refs": 2}, "SPEC.md": {"rehome": 11, "local_refs": 1}})
    );
}

#[test]
fn init_holds_each_files_refs_at_adoption_so_check_is_green_that_day() {
    let toml = "local_issue_refs = [\"rollmind\"]\n";
    let dir = repo(&[
        ("docfit.toml", toml),
        ("README.md", "r\n"),
        ("src/a.rs", "// rollmind#1 rollmind#2\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["baseline"], json!({"src/a.rs": {"local_refs": 2}}));
    assert_eq!(check(dir.path()).0, 0);
}

#[test]
fn init_on_a_baseline_holding_no_refs_adopts_them_and_once_held_leaves_them() {
    let dir = repo(&[
        ("docfit.toml", ON),
        (
            ".docfit/baseline.json",
            "{\"README.md\": {\"bytes\": 40}}\n",
        ),
        ("README.md", "r\n"),
        ("src/a.rs", "// rollmind#1\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], true);
    assert_eq!(
        answer["baseline"],
        json!({"README.md": {"bytes": 40}, "src/a.rs": {"local_refs": 1}})
    );
    std::fs::write(dir.path().join("src/b.rs"), "// rollmind#2\n").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    let (_, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(answer["changed"], false, "{answer}");
    assert_eq!(answer["faults"][0]["fault"], "local_ref");
}

const RULE: &str = "[documents]\n\"README.md\" = \"readme\"\n";

#[test]
fn by_the_rule_a_ref_to_any_tracker_in_a_comment_is_red_with_no_list() {
    let dir = repo(&[
        ("docfit.toml", RULE),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("src/a.rs", "fn a() {}\n// as guild#66 asked\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let fault = &answer["faults"][0];
    assert_eq!(
        (
            &fault["file"],
            &fault["fault"],
            &fault["line"],
            &fault["ref"]
        ),
        (
            &json!("src/a.rs"),
            &json!("local_ref"),
            &json!(2),
            &json!("guild#66")
        )
    );
    assert!(
        fault["fix"]
            .as_str()
            .unwrap()
            .contains("docfit help local-refs"),
        "{fault}"
    );
}

#[test]
fn by_the_rule_an_owner_a_url_a_symbol_a_path_or_a_capital_is_no_ref() {
    let text = "sunfmin/kiln#25 https://x/issues/3 {closure#0} a.md#3 C#1\n";
    let dir = repo(&[
        ("docfit.toml", RULE),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("src/a.rs", text),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn a_name_in_not_issue_refs_is_no_ref() {
    let toml = format!("not_issue_refs = [\"shim\"]\n{RULE}");
    let dir = repo(&[
        ("docfit.toml", &toml),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("src/a.rs", "// shim#2 is a symbol\n"),
    ]);
    assert_eq!(check(dir.path()).0, 0);
    std::fs::write(dir.path().join("src/a.rs"), "// shim#2, kiln#3\n").unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!((code, &answer["faults"][0]["ref"]), (1, &json!("kiln#3")));
}

#[test]
fn a_not_issue_refs_that_is_not_a_list_of_names_is_refused() {
    for bad in [
        "not_issue_refs = \"shim\"",
        "not_issue_refs = [\"Shim\"]",
        "not_issue_refs = [\"\"]",
    ] {
        let toml = format!("{bad}\n{RULE}");
        let dir = repo(&[("docfit.toml", &toml), ("README.md", "r\n")]);
        let (code, answer) = check(dir.path());
        assert_eq!(code, 2, "{bad}: {answer}");
        let fix = answer["error"][0]["fix"].as_str().unwrap();
        assert!(fix.contains("not_issue_refs = [\"<name>\", ...]"), "{fix}");
    }
}

#[test]
fn init_without_a_config_writes_no_list_and_holds_each_files_refs_by_the_rule() {
    let dir = repo(&[
        ("README.md", "r\n"),
        ("src/a.rs", "// guild#66 kiln#1 sunfmin/kiln#2\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["baseline"], json!({"src/a.rs": {"local_refs": 2}}));
    let toml = std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap();
    assert!(!toml.contains("local_issue_refs"), "{toml}");
    assert_eq!(check(dir.path()).0, 0);
}

/// A repo docfit 0.1.3 adopted: `local_issue_refs` listed, under a comment
/// naming it, and its baseline holding the listed refs only, beside a ref to
/// a checkout the list lacks and a fixture's made-up one.
fn upgraded() -> tempfile::TempDir {
    let toml = "# Docs.\n# local_issue_refs: the checkouts when docfit was wired in.\nlocal_issue_refs = [\"mill\"]\n\n[documents]\n\"README.md\" = { kind = \"readme\", bytes = 10 }\n";
    let held =
        "{\"README.md\": {\"bytes\": 37, \"local_refs\": 1}, \"src/a.rs\": {\"local_refs\": 2}}\n";
    repo(&[
        ("docfit.toml", toml),
        (".docfit/baseline.json", held),
        ("README.md", "mill#1 xxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n"),
        ("src/a.rs", "// mill#1 mill#2 guild#66\n"),
        ("tests/data.rs", "const T: &str = \"proj#7\";\n"),
    ])
}

#[test]
fn a_repo_on_local_issue_refs_is_green_the_day_it_upgrades_and_next_asks_to_drop_it() {
    let dir = upgraded();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    let next = answer["next"].as_str().unwrap();
    assert!(next.contains("docfit ratchet --local-refs"), "{next}");
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(answer["changed"], false, "{answer}");

    let (code, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["counts"]["drop-local-issue-refs"], 1, "{answer}");
    let task = &answer["tasks"][0];
    assert_eq!(task["kind"], "drop-local-issue-refs");
    assert_eq!(task["files"], json!(["docfit.toml"]));
    assert_eq!(task["evidence"], json!({"local_issue_refs": ["mill"]}));
    let steps = task["steps"].as_str().unwrap();
    assert!(
        steps.contains("docfit ratchet --local-refs") && !steps.contains("{const:"),
        "{steps}"
    );
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_docfit"))
        .args(["help", "drop-local-issue-refs"])
        .output()
        .unwrap();
    assert!(String::from_utf8(help.stdout).unwrap().contains(steps));
}

#[test]
fn the_drop_tasks_steps_delete_the_list_hold_the_rules_counts_once_and_stay_green() {
    let dir = upgraded();
    let (code, answer) = docfit(dir.path(), &["ratchet", "--local-refs"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], true);
    assert_eq!(
        answer["baseline"],
        json!({"README.md": {"bytes": 37, "local_refs": 1}, "src/a.rs": {"local_refs": 3}, "tests/data.rs": {"local_refs": 1}})
    );
    let toml = std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap();
    assert_eq!(
        toml,
        "# Docs.\n\n[documents]\n\"README.md\" = { kind = \"readme\", bytes = 10 }\n"
    );
    assert_eq!(check(dir.path()).0, 0);

    // Step 2: the fixture's made-up ref is test data, ignored by glob.
    let ignored = format!("ignore = [\"tests/**\"]\n{toml}");
    std::fs::write(dir.path().join("docfit.toml"), ignored).unwrap();
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        answer["baseline"],
        json!({"README.md": {"bytes": 37, "local_refs": 1}, "src/a.rs": {"local_refs": 3}})
    );
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    let (_, answer) = docfit(dir.path(), &["next", "--all"]);
    assert_eq!(answer["counts"]["drop-local-issue-refs"], 0, "{answer}");

    // Once: without the list, nothing may move a count up again.
    std::fs::write(
        dir.path().join("src/a.rs"),
        "// guild#1 guild#2 guild#3 guild#4\n",
    )
    .unwrap();
    let (code, answer) = docfit(dir.path(), &["ratchet", "--local-refs"]);
    assert_eq!(code, 2, "{answer}");
    assert_eq!(answer["error"][0]["fix"], "run `docfit ratchet`");
    assert_eq!(check(dir.path()).0, 1);
}

#[test]
fn a_binary_file_is_not_read() {
    let dir = repo(&[
        ("docfit.toml", ON),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("a.bin", "\0rollmind#1\n"),
    ]);
    assert_eq!(check(dir.path()).0, 0);
}

#[test]
fn a_tracked_file_gone_from_the_working_tree_is_not_read() {
    let dir = repo(&[
        ("docfit.toml", ON),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("src/gone.rs", "// rollmind#1\n"),
    ]);
    std::fs::remove_file(dir.path().join("src/gone.rs")).unwrap();
    assert_eq!(check(dir.path()).0, 0);
}

#[test]
fn ratchet_local_refs_on_a_list_written_other_than_as_its_line_is_refused() {
    let toml = format!("\"local_issue_refs\" = [\"mill\"]\n{RULE}");
    let dir = repo(&[("docfit.toml", &toml), ("README.md", "r\n")]);
    let (code, answer) = docfit(dir.path(), &["ratchet", "--local-refs"]);
    assert_eq!(code, 2, "{answer}");
    let fix = answer["error"][0]["fix"].as_str().unwrap();
    assert!(
        fix.contains("as a line `local_issue_refs = [...]`"),
        "{fix}"
    );
    let kept = std::fs::read_to_string(dir.path().join("docfit.toml")).unwrap();
    assert_eq!(kept, toml);
}
