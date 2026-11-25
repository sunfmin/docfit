//! `docfit where` on a throwaway git repo: each paragraph's features, its
//! suggested home, and what it refuses.

mod common;

use common::{docfit, repo};
use serde_json::{Value, json};

const CONFIG: &str = "catalogs = [\"App/**/*.xcstrings\"]\n\n[documents]\n\"SPEC.md\" = { kind = \"readme\", paragraph = 400 }\n\"NOTES.md\" = \"readme\"\n\"docs/adr/*.md\" = \"adr\"\n\"docs/agents/*.md\" = \"agent\"\n";

const CATALOG: &str = r#"{ "strings": { "save": { "localizations": { "zh-Hans": { "stringUnit": { "value": "保存" } }, "en": { "stringUnit": { "value": "Cancel" } } } } } }"#;

const SPEC: &str = "# Spec

The answer is built by `Answer` and read back by `parse_config`, while `Missing` and `docfit check --help` are not code, `Answer` said twice is one, `answer::Answer` is code but `answer::Answ` is no whole word, and the words here run on long enough.

Tap 「保存」 and then \"Cancel\" to leave the page; \"Nowhere\" is a string no catalog holds, so it does not count, nor does a stray “ quote, nor an empty \"\".

Startup took 1.5 s and 63 MiB (#12, rollmind#146), down from 70 分钟, at 100% of 112×112 tiles; #notes and #12a are no issue.

We parse the table by hand instead of with a crate, because the build stays small.

We chose globs instead of regular expressions because ADR 0002 says so.

See [the tracker](docs/agents/tracker.md).

This paragraph [sic] says nothing a machine can see.

- one item restating `Answer`
  and its second line
- 因为快，所以选 glob 而不是正则。

| what | took |
|---|---|
| a row | 3 ms |

```rust
let x = \"Cancel\";
```
";

fn fixture() -> tempfile::TempDir {
    repo(&[
        ("docfit.toml", CONFIG),
        ("SPEC.md", SPEC),
        ("App/Base/Localizable.xcstrings", CATALOG),
        (
            "src/answer.rs",
            "use crate::answer::Answer;\nstruct Answer;\nfn parse_config() {}\n// docfit check --help\n",
        ),
        (
            "docs/adr/0002-globs-for-documents.md",
            "# Globs for documents\n",
        ),
        ("docs/agents/tracker.md", "x\n"),
        (
            "NOTES.md",
            "`Missing` lives only in Markdown, which is not code.\n",
        ),
    ])
}

fn paragraphs(answer: &Value) -> &Vec<Value> {
    answer["paragraphs"].as_array().unwrap()
}

#[test]
fn each_paragraph_gets_its_features_and_its_suggested_home() {
    let dir = fixture();
    let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["file"], "SPEC.md");
    let p = paragraphs(&answer);
    let lines: Vec<_> = p.iter().map(|p| p["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [3, 5, 7, 9, 11, 13, 15, 17, 19, 23, 25], "{answer}");

    assert_eq!(
        p[0]["code"],
        json!({ "count": 3, "found": ["Answer", "parse_config", "answer::Answer"] })
    );
    assert_eq!(p[0]["home"], "delete");
    assert!(
        p[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("The answer is built")
    );

    assert_eq!(
        p[1]["strings"],
        json!({ "count": 2, "found": ["保存", "Cancel"] })
    );
    assert_eq!(p[1]["home"], "delete");

    assert_eq!(p[2]["evidence"], json!(["1.5 s", "63 MiB", "70 分钟"]));
    assert_eq!(p[2]["local_refs"], json!(["#12", "rollmind#146"]));
    assert_eq!(p[2]["home"], "evidence");

    assert_eq!(p[3]["decision"], json!(["because", "instead of"]));
    assert_eq!(p[3]["home"], "adr");
    assert_eq!(p[3]["adr"], Value::Null);
    assert_eq!(p[4]["home"], "adr");
    assert_eq!(p[4]["adr"], "docs/adr/0002-globs-for-documents.md");

    assert_eq!(p[5]["pointer"], true);
    assert_eq!(p[5]["home"], "keep");

    assert_eq!(p[6]["home"], "judge");
    for feature in ["evidence", "local_refs", "decision"] {
        assert_eq!(p[6][feature], json!([]), "{feature}");
    }
    assert_eq!(p[6]["code"], json!({ "count": 0, "found": [] }));
    assert_eq!(p[6]["pointer"], false);

    // A list item is one paragraph, its continuation line with it.
    assert_eq!(p[7]["home"], "delete");
    assert_eq!(p[8]["decision"], json!(["因为", "选", "而不是"]));
    // A table row is one paragraph; its header and separator are not.
    assert_eq!(p[9]["evidence"], json!(["3 ms"]));
    assert_eq!(p[9]["home"], "evidence");
    // A fenced block is one paragraph, nothing read inside it.
    assert_eq!(p[10]["home"], "judge");
    assert_eq!(p[10]["strings"]["count"], 0);

    assert_eq!(
        answer["homes"],
        json!({ "keep": 1, "adr": 3, "delete": 3, "evidence": 2, "replace": 0, "judge": 2 })
    );
    assert!(answer["next"].as_str().unwrap().contains("docfit check"));
}

#[test]
fn a_pointer_may_name_a_path_in_backticks() {
    let dir = fixture();
    std::fs::write(
        dir.path().join("SPEC.md"),
        "Issues: see `docs/agents/tracker.md`.\n\nTwo sentences here. See `docs/agents/tracker.md`.\n\n`Answer` is checked against `docs/agents/tracker.md`.\n\n[`SPEC.md`](SPEC.md)\n",
    )
    .unwrap();
    let (_, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
    let p = paragraphs(&answer);
    assert_eq!(p[0]["home"], "keep", "{answer}");
    assert_eq!(p[0]["code"]["found"], json!(["docs/agents/tracker.md"]));
    assert_eq!(p[1]["pointer"], false, "{answer}");
    // Short, with a path, but not saying where anything is.
    assert_eq!(p[2]["pointer"], false, "{answer}");
    // Only a link: nothing else to say.
    assert_eq!(p[3]["home"], "keep", "{answer}");
}

#[test]
fn it_runs_from_any_directory_and_changes_no_file() {
    let dir = fixture();
    let status = || {
        let out = std::process::Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=all"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let before = status();
    let (code, answer) = docfit(&dir.path().join("docs/agents"), &["where", "../../SPEC.md"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["file"], "SPEC.md");
    assert_eq!(answer["kind"], "readme");
    assert_eq!(status(), before);
}

#[test]
fn a_document_it_cannot_read_is_refused_with_its_fix() {
    let dir = fixture();
    let (code, answer) = docfit(dir.path(), &["where", "nope.md"]);
    assert_eq!(code, 2, "{answer}");
    assert!(
        answer["error"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("a document in a git repository"),
        "{answer}"
    );
}

#[test]
fn catalogs_it_cannot_read_are_refused_with_their_fix() {
    // `check` reads the same config: it refuses what is not one too.
    for (catalogs, fix, check) in [
        ("catalogs = \"App\"", "a list of globs", 2),
        ("catalogs = [\"Nothing/*.xcstrings\"]", "matches no file", 0),
        ("catalogs = [\"[\"]", "fix `[`", 2),
    ] {
        let dir = fixture();
        let config = CONFIG.replace("catalogs = [\"App/**/*.xcstrings\"]", catalogs);
        std::fs::write(dir.path().join("docfit.toml"), config).unwrap();
        let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
        assert_eq!(code, 2, "{catalogs}: {answer}");
        let all = answer["error"].to_string();
        assert!(all.contains(fix), "{catalogs}: {answer}");
        let (code, answer) = docfit(dir.path(), &["check"]);
        assert_eq!(code, check, "{catalogs}: {answer}");
    }
}

#[test]
fn without_catalogs_no_string_is_found() {
    let dir = fixture();
    let config = CONFIG.replace("catalogs = [\"App/**/*.xcstrings\"]\n", "");
    std::fs::write(dir.path().join("docfit.toml"), config).unwrap();
    let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(paragraphs(&answer)[1]["strings"]["count"], 0);
}

#[test]
fn an_empty_document_has_nothing_to_place() {
    let dir = fixture();
    std::fs::write(dir.path().join("SPEC.md"), "# Only a heading\n").unwrap();
    let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["paragraphs"], json!([]));
    assert_eq!(
        answer["next"],
        "nothing to place: `SPEC.md` has no paragraph"
    );
}

#[test]
fn a_catalog_or_an_adr_it_cannot_read_is_refused_with_its_fix() {
    for file in [
        "App/Base/Localizable.xcstrings",
        "docs/adr/0002-globs-for-documents.md",
    ] {
        let dir = fixture();
        std::fs::write(dir.path().join(file), [0xff, 0xfe]).unwrap();
        let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
        assert_eq!(code, 2, "{file}: {answer}");
        assert_eq!(
            answer["error"][0]["fix"],
            format!("restore `{file}` as UTF-8 text, or `git rm` it")
        );
    }
}

#[test]
fn the_edges_of_each_feature() {
    let dir = fixture();
    let long = "x".repeat(101);
    let catalog = format!(r#"{{ "a": "{long}", "b": "Cancel" }}"#);
    std::fs::write(dir.path().join("App/Base/Localizable.xcstrings"), catalog).unwrap();
    let spec = format!(
        "Only a reason, because it is so.\n\nOnly an alternative, instead of that.\n\nWe chose them instead of others because of globs for documents, as decided.\n\n1. first\n2. second\n\nA catalog string past 100 characters is not one: \"{long}\", but \"Cancel\" is.\n\nShort.\n\nFixed in rollmind#7, as #8 says.\n"
    );
    std::fs::write(dir.path().join("SPEC.md"), spec).unwrap();
    let (code, answer) = docfit(dir.path(), &["where", "SPEC.md"]);
    assert_eq!(code, 0, "{answer}");
    let p = paragraphs(&answer);
    assert_eq!(p.len(), 8, "{answer}");
    assert_eq!(p[0]["decision"], json!([]));
    assert_eq!(p[1]["decision"], json!([]));
    assert_eq!(
        p[2]["adr"], "docs/adr/0002-globs-for-documents.md",
        "by its title"
    );
    assert_eq!(
        (p[3]["text"].clone(), p[4]["text"].clone()),
        (json!("1. first"), json!("2. second"))
    );
    assert_eq!(p[5]["strings"], json!({ "count": 1, "found": ["Cancel"] }));
    assert!(p[5]["text"].as_str().unwrap().ends_with('…'), "{answer}");
    assert_eq!(p[5]["text"].as_str().unwrap().chars().count(), 81);
    assert_eq!(p[6]["text"], "Short.");
    // An issue of the local tracker is its own feature: what it points at replaces it.
    assert_eq!(p[7]["local_refs"], json!(["rollmind#7", "#8"]));
    assert_eq!(p[7]["evidence"], json!([]));
    assert_eq!(p[7]["home"], "replace");
}
