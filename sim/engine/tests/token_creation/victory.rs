use super::*;

fn selection(steps: &Value) -> (String, String, Value) {
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![json!({"spell":{"do":steps}})],
    );
    let (member, template) = author(
        "member",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![],
    );
    (
        format!("{creator}\n{member}\n"),
        json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string(),
        json!({"documents":[],"authors":[source,template]}),
    )
}

#[test]
fn victory_and_any_discard_keep_unsupported_author_variants_closed() {
    for step in [
        json!({"win":{"who":"opp"}}),
        json!({"win":{"who":"self","result":"won"}}),
        json!({"discard":{"any":true,"who":"opp"}}),
        json!({"discard":{"any":true,"n":1_u8}}),
        json!({"discard":{"any":true,"random":true}}),
    ] {
        let (snapshot, keywords, input) = selection(&json!([step]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
    let (snapshot, keywords, input) = selection(&json!([
        {"discard":{"any":true,"result":"discarded"}},
        {"move":{"ref":{"in":{"objects_of":"discarded","view":"before"}},"to":"deck","position":"bottom","order":"chooser"}}
    ]));
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
}
