use super::*;

fn found(text: &str, rule: &Rule) -> Vec<(usize, String)> {
    find(text, rule)
        .into_iter()
        .map(|r| (r.line, r.text))
        .collect()
}

fn listed(text: &str) -> Vec<(usize, String)> {
    let names = vec!["rollmind".to_string(), "lore".to_string()];
    found(text, &Rule::Listed(names))
}

fn refs(text: &str) -> Vec<(usize, String)> {
    found(text, &Rule::Unlisted(vec!["closure".to_string()]))
}

#[test]
fn a_listed_ref_is_a_listed_name_then_hash_digits() {
    assert_eq!(
        listed("x\n(rollmind#38), lore#7. guild#66\n"),
        [(2, "rollmind#38".into()), (2, "lore#7".into())]
    );
    assert_eq!(listed("`rollmind#1`"), [(1, "rollmind#1".into())]);
    assert_eq!(
        listed("https://x.example/a rollmind#2 (rollmind#3"),
        [(1, "rollmind#2".into()), (1, "rollmind#3".into())]
    );
}

#[test]
fn an_owner_a_url_a_longer_name_or_a_word_after_is_no_listed_ref() {
    for text in [
        "sunfmin/lore#25",
        "https://x.example/rollmind#3",
        "https://github.com/sunfmin/rollmind/issues/3",
        "https://x.example/?q=rollmind#3",
        "xrollmind#4 my-lore#5 a.lore#6",
        "rollmind#38x rollmind#3_ rollmind# #12",
    ] {
        assert_eq!(listed(text), [], "{text}");
    }
}

#[test]
fn by_the_rule_any_lowercase_name_then_hash_digits_is_a_ref() {
    assert_eq!(
        refs("x\n// (guild#66), my-tool_2#7. `kiln#1`\n"),
        [
            (2, "guild#66".into()),
            (2, "my-tool_2#7".into()),
            (2, "kiln#1".into())
        ]
    );
    assert_eq!(refs("https://x.example/a kiln#2"), [(1, "kiln#2".into())]);
}

#[test]
fn by_the_rule_an_owner_a_url_a_path_a_symbol_or_a_named_non_tracker_is_no_ref() {
    for text in [
        "sunfmin/kiln#25",
        "https://x/issues/3 https://x.example/kiln#3 https://x.example/?q=kiln#3",
        "{closure#0} {shim#1} a.md#3 C#1 Lore#4 x:kiln#5",
        "closure#2",
        "kiln#38x kiln#3_ kiln# #12 _x#1 -x#1 9x#1",
        "日本kiln#1",
    ] {
        assert_eq!(refs(text), [], "{text}");
    }
}
