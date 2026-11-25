//! `docfit check`, `ratchet` and `init` on local tracker refs (`local_issue_refs`):
//! their JSON and exit code.

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
}

#[test]
fn a_ref_with_an_owner_or_in_a_url_is_no_ref() {
    let text = "sunfmin/lore#25\nhttps://github.com/sunfmin/rollmind/issues/3\n<https://x.example/q?rollmind#3>\nrollmind#38x and rollmind# and xrollmind#4 and my-lore#5\n";
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
        (".docfit/baseline.json", "{}\n"),
        ("README.md", "r\n"),
        ("src/a.rs", "// rollmind#1\n"),
    ]);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["changed"], true);
    assert_eq!(answer["baseline"], json!({"src/a.rs": {"local_refs": 1}}));
    std::fs::write(dir.path().join("src/b.rs"), "// rollmind#2\n").unwrap();
    common::git(dir.path(), &["add", "-A"]);
    let (_, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(answer["changed"], false, "{answer}");
    assert_eq!(answer["faults"][0]["fault"], "local_ref");
}

#[test]
fn without_local_issue_refs_the_check_is_off_and_next_says_how_to_turn_it_on() {
    let dir = repo(&[
        ("docfit.toml", "[documents]\n\"README.md\" = \"readme\"\n"),
        (
            ".docfit/baseline.json",
            "{\"src/a.rs\": {\"local_refs\": 1}}\n",
        ),
        ("README.md", "r\n"),
        ("src/a.rs", "// rollmind#1 rollmind#2\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    let next = answer["next"].as_str().unwrap();
    assert!(next.contains("local_issue_refs = ["), "{next}");
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(answer["changed"], false, "{answer}");
}

#[test]
fn a_local_issue_refs_that_is_not_a_list_of_names_is_refused() {
    for bad in [
        "local_issue_refs = \"rollmind\"",
        "local_issue_refs = [\"a b\"]",
        "local_issue_refs = [\"\"]",
    ] {
        let toml = format!("{bad}\n[documents]\n\"README.md\" = \"readme\"\n");
        let dir = repo(&[("docfit.toml", &toml), ("README.md", "r\n")]);
        let (code, answer) = check(dir.path());
        assert_eq!(code, 2, "{bad}: {answer}");
        let fix = answer["error"][0]["fix"].as_str().unwrap();
        assert!(
            fix.contains("local_issue_refs = [\"<name>\", ...]"),
            "{fix}"
        );
    }
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
