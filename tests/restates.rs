//! `docfit check`, `ratchet`, `init` and `where` on restatements: a quoted
//! string a catalog holds, and a run of lines copied from another file.

mod common;

use common::{docfit, repo};
use serde_json::{Value, json};
use std::path::Path;

const CONFIG: &str = "catalogs = [\"App/**/*.xcstrings\"]\n[documents]\n\"README.md\" = \"readme\"\n\"NOTES.md\" = \"readme\"\n";

const CATALOG: &str = r#"{ "strings": { "save": { "localizations": { "en": { "stringUnit": { "value": "Save photo" } } } } } }"#;

const CODE: &str =
    "fn main() {\n    let answer = check(repo);\n    print(answer);\n    exit(code);\n}\n";

fn fixture(readme: &str) -> tempfile::TempDir {
    repo(&[
        ("docfit.toml", CONFIG),
        (".docfit/baseline.json", "{}\n"),
        ("App/Base/Localizable.xcstrings", CATALOG),
        ("src/a.rs", CODE),
        ("NOTES.md", "n\n"),
        ("README.md", readme),
    ])
}

fn check(dir: &Path) -> (i32, Value) {
    docfit(dir, &["check"])
}

fn restates(answer: &Value) -> Vec<&Value> {
    answer["faults"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["fault"] == "restates")
        .collect()
}

#[test]
fn a_quoted_catalog_string_is_red_naming_its_line_and_catalog() {
    let dir = fixture("# R\n\nTap \"Save photo\" to keep it; \"Other\" is no catalog's.\n");
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = restates(&answer);
    assert_eq!(faults.len(), 1, "{answer}");
    let f = faults[0];
    assert_eq!(
        (
            &f["file"],
            &f["restates"],
            &f["line"],
            &f["found"],
            &f["from"]
        ),
        (
            &json!("README.md"),
            &json!("strings"),
            &json!(3),
            &json!("Save photo"),
            &json!("App/Base/Localizable.xcstrings")
        )
    );
    assert_eq!((&f["size"], &f["limit"]), (&json!(1), &json!(0)));
    let fix = f["fix"].as_str().unwrap();
    assert!(
        fix.contains("point at the key, or at the screen test that shows it, instead"),
        "{fix}"
    );
}

#[test]
fn three_lines_copied_from_code_are_red_and_two_are_green() {
    let three = "# R\n\nIt runs:\n\nlet answer = check(repo);\nprint(answer);\nexit(code);\n";
    let dir = fixture(three);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = restates(&answer);
    assert_eq!(faults.len(), 1, "{answer}");
    let f = faults[0];
    assert_eq!(
        (&f["restates"], &f["line"], &f["from"], &f["size"]),
        (&json!("copies"), &json!(5), &json!("src/a.rs:2"), &json!(1))
    );
    let fix = f["fix"].as_str().unwrap();
    assert!(
        fix.contains("keep it in one place and point at it"),
        "{fix}"
    );

    let two = "# R\n\nlet answer = check(repo);\nprint(answer);\n\nexit(code);\n";
    let dir = fixture(two);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "blank lines between are skipped: {answer}");
    let two = "# R\n\nlet answer = check(repo);\nprint(answer);\nsomething else\n";
    let dir = fixture(two);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn a_fence_naming_its_source_is_no_copy_but_one_naming_another_is() {
    let named = "# R\n\nFrom `src/a.rs`:\n\n```rust\nlet answer = check(repo);\nprint(answer);\nexit(code);\n```\n";
    let (code, answer) = check(fixture(named).path());
    assert_eq!(code, 0, "{answer}");
    let info = "# R\n\n```rust a.rs\nlet answer = check(repo);\nprint(answer);\nexit(code);\n```\n";
    let (code, answer) = check(fixture(info).path());
    assert_eq!(code, 0, "{answer}");
    for other in ["src/b.rs", "data.rs"] {
        let other = named.replace("src/a.rs", other);
        let (code, answer) = check(fixture(&other).path());
        assert_eq!(code, 1, "{other}: {answer}");
    }
}

#[test]
fn a_run_another_document_holds_is_red_on_both() {
    let text =
        "# R\n\nThe first line of a run.\nThe second line of a run.\nThe third line of a run.\n";
    let dir = fixture(text);
    std::fs::write(dir.path().join("NOTES.md"), text).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let files: Vec<_> = restates(&answer)
        .iter()
        .map(|f| f["file"].clone())
        .collect();
    assert_eq!(files, [json!("NOTES.md"), json!("README.md")], "{answer}");
}

#[test]
fn held_counts_are_green_until_they_grow_and_ratchet_only_lowers_them() {
    let both = "# R\n\nTap \"Save photo\", then \"Save photo\".\n\nlet answer = check(repo);\nprint(answer);\nexit(code);\n";
    let dir = fixture(both);
    std::fs::remove_file(dir.path().join(".docfit/baseline.json")).unwrap();
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["baseline"],
        json!({ "README.md": { "strings": 2, "copies": 1 } })
    );
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");

    let grown = format!("{both}\nAnd \"Save photo\" again.\n");
    std::fs::write(dir.path().join("README.md"), &grown).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let f = restates(&answer)[0];
    assert_eq!(
        (&f["restates"], &f["line"], &f["size"], &f["limit"]),
        (&json!("strings"), &json!(9), &json!(3), &json!(2))
    );

    let cut = "# R\n\nTap \"Save photo\".\n";
    std::fs::write(dir.path().join("README.md"), cut).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert!(answer["next"].as_str().unwrap().contains("docfit ratchet"));
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        answer["baseline"],
        json!({ "README.md": { "strings": 1 } }),
        "{answer}"
    );
    std::fs::write(dir.path().join("README.md"), both).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "a count once lowered is not raised: {answer}");
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(answer["baseline"], json!({ "README.md": { "strings": 1 } }));
}

#[test]
fn init_holds_restatements_on_a_baseline_that_holds_none_yet() {
    let dir = fixture("# R\n\nTap \"Save photo\".\n");
    let (code, _) = check(dir.path());
    assert_eq!(code, 1);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["baseline"], json!({ "README.md": { "strings": 1 } }));
    assert_eq!(check(dir.path()).0, 0);
    // Once held, a run of init adds none.
    std::fs::write(dir.path().join("NOTES.md"), "Tap \"Save photo\" too.\n").unwrap();
    let (_, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(answer["baseline"], json!({ "README.md": { "strings": 1 } }));
}

#[test]
fn init_holds_restatements_beside_an_unrelated_hold() {
    let dir = fixture("# R\n\nTap \"Save photo\".\n");
    std::fs::write(
        dir.path().join(".docfit/baseline.json"),
        "{\"NOTES.md\": {\"local_refs\": 1}}\n",
    )
    .unwrap();
    let (_, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(
        answer["baseline"]["README.md"],
        json!({ "strings": 1 }),
        "{answer}"
    );
}

#[test]
fn where_lists_what_check_counts() {
    let both =
        "# R\n\nTap \"Save photo\".\n\nlet answer = check(repo);\nprint(answer);\nexit(code);\n";
    let dir = fixture(both);
    let (_, answer) = docfit(dir.path(), &["where", "README.md"]);
    let p = answer["paragraphs"].as_array().unwrap();
    assert_eq!(
        p[0]["strings"],
        json!({ "count": 1, "found": ["Save photo"] })
    );
    assert_eq!(
        p[1]["copies"],
        json!({ "count": 1, "found": ["src/a.rs:2"] })
    );
    assert_eq!(p[0]["copies"], json!({ "count": 0, "found": [] }));
}
