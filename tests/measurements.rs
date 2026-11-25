//! `docfit check`, `ratchet` and `init` on measurements: a number with a
//! unit in a readme, an agent doc or a skill.

mod common;

use common::{docfit, repo};
use serde_json::{Value, json};
use std::path::Path;

const CONFIG: &str = "[documents]\n\"README.md\" = \"readme\"\n\"docs/adr/*.md\" = \"adr\"\n";

fn fixture(readme: &str) -> tempfile::TempDir {
    repo(&[
        ("docfit.toml", CONFIG),
        (".docfit/baseline.json", "{}\n"),
        ("README.md", readme),
        ("docs/adr/0001-x.md", "# X\n\nWe chose it.\n"),
    ])
}

fn check(dir: &Path) -> (i32, Value) {
    docfit(dir, &["check"])
}

fn measurements(answer: &Value) -> Vec<&Value> {
    answer["faults"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["fault"] == "measurement")
        .collect()
}

#[test]
fn a_number_with_a_unit_in_a_readme_is_red_naming_its_line() {
    let dir = fixture("# R\n\nThe gate runs in 2 min.\n");
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let faults = measurements(&answer);
    assert_eq!(faults.len(), 1, "{answer}");
    let f = faults[0];
    assert_eq!(
        (&f["file"], &f["line"], &f["found"], &f["size"], &f["limit"]),
        (
            &json!("README.md"),
            &json!(3),
            &json!("2 min"),
            &json!(1),
            &json!(0)
        )
    );
    let fix = f["fix"].as_str().unwrap();
    assert!(
        fix.contains("move it to the ADR of the decision it supports, with the command that measures it, or to the issue that accepted the work; or delete it"),
        "{fix}"
    );
    for unit in [
        "300 ms",
        "1.5s",
        "4 h",
        "70 分钟",
        "3 小时",
        "63 MiB",
        "2GB",
        "40%",
    ] {
        let dir = fixture(&format!("# R\n\nIt took {unit}.\n"));
        let (code, answer) = check(dir.path());
        assert_eq!(code, 1, "{unit}: {answer}");
        assert_eq!(measurements(&answer)[0]["found"], unit);
    }
}

#[test]
fn the_same_sentence_in_an_adr_is_green() {
    let dir = fixture("# R\n");
    std::fs::write(
        dir.path().join("docs/adr/0001-x.md"),
        "# X\n\nThe gate runs in 2 min.\n",
    )
    .unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn a_limit_in_code_or_a_number_without_a_unit_is_green() {
    let text = "# R\n\n```toml\nfast_limit = 300\ntook = \"2 min\"\n```\n\nSet `fast_limit = 300 ms`; step 2 of 3, v1.5s, in 2026.\n";
    let (code, answer) = check(fixture(text).path());
    assert_eq!(code, 0, "{answer}");
}

#[test]
fn held_counts_are_green_until_they_grow_and_ratchet_only_lowers_them() {
    let two = "# R\n\nIt runs in 2 min, in 63 MiB.\n";
    let dir = fixture(two);
    std::fs::remove_file(dir.path().join(".docfit/baseline.json")).unwrap();
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["baseline"],
        json!({ "README.md": { "measurements": 2 } })
    );
    assert_eq!(check(dir.path()).0, 0);

    let grown = format!("{two}\nAnd 40% faster.\n");
    std::fs::write(dir.path().join("README.md"), &grown).unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let f = measurements(&answer)[0];
    assert_eq!(
        (&f["line"], &f["found"], &f["size"], &f["limit"]),
        (&json!(5), &json!("40%"), &json!(3), &json!(2))
    );

    std::fs::write(dir.path().join("README.md"), "# R\n\nIt runs in 2 min.\n").unwrap();
    let (code, answer) = check(dir.path());
    assert_eq!(code, 0, "{answer}");
    assert!(answer["next"].as_str().unwrap().contains("docfit ratchet"));
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(
        answer["baseline"],
        json!({ "README.md": { "measurements": 1 } })
    );
    std::fs::write(dir.path().join("README.md"), two).unwrap();
    assert_eq!(check(dir.path()).0, 1, "a count once lowered is not raised");
    std::fs::write(dir.path().join("README.md"), "# R\n").unwrap();
    let (_, answer) = docfit(dir.path(), &["ratchet"]);
    assert_eq!(answer["baseline"], json!({}), "dropped at 0");
}

#[test]
fn init_holds_measurements_on_a_baseline_that_holds_none_yet_and_only_once() {
    let dir = fixture("# R\n\nIt runs in 2 min.\n");
    std::fs::write(
        dir.path().join(".docfit/baseline.json"),
        "{\"docs/adr/0001-x.md\": {\"local_refs\": 1}}\n",
    )
    .unwrap();
    assert_eq!(check(dir.path()).0, 1);
    let (code, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        answer["baseline"],
        json!({ "README.md": { "measurements": 1 }, "docs/adr/0001-x.md": { "local_refs": 1 } })
    );
    assert_eq!(check(dir.path()).0, 0);
    std::fs::write(dir.path().join("README.md"), "# R\n\nIn 2 min, 3 MB.\n").unwrap();
    let (_, answer) = docfit(dir.path(), &["init"]);
    assert_eq!(
        answer["baseline"]["README.md"],
        json!({ "measurements": 1 })
    );
}

#[test]
fn an_agent_doc_and_a_skill_are_checked_and_where_finds_the_same() {
    let dir = repo(&[
        (
            "docfit.toml",
            "[documents]\n\"CLAUDE.md\" = \"agent\"\n\"skills/a/SKILL.md\" = \"skill\"\n\"GLOSSARY.md\" = \"glossary\"\n",
        ),
        (".docfit/baseline.json", "{}\n"),
        ("CLAUDE.md", "Tests take 9 s.\n"),
        ("skills/a/SKILL.md", "It needs 6 GiB.\n"),
        ("GLOSSARY.md", "A run is 2 min.\n"),
    ]);
    let (code, answer) = check(dir.path());
    assert_eq!(code, 1, "{answer}");
    let files: Vec<_> = measurements(&answer)
        .iter()
        .map(|f| (f["file"].clone(), f["found"].clone()))
        .collect();
    assert_eq!(
        files,
        [
            (json!("CLAUDE.md"), json!("9 s")),
            (json!("skills/a/SKILL.md"), json!("6 GiB"))
        ]
    );
    let (_, answer) = docfit(dir.path(), &["where", "CLAUDE.md"]);
    assert_eq!(answer["paragraphs"][0]["evidence"], json!(["9 s"]));
}
