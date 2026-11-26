use super::*;

fn inputs(steps: &Value) -> (String, String, Value) {
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
        json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string(),
        json!({"documents":[],"authors":[source,template]}),
    )
}

fn steps() -> Value {
    json!([
        {"look":{"bind":"seen","top":{"who":"self","n":5_u8}}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2_u8,"may":true}},
        {"banish":{"ref":"picked","result":"banished"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"banished","view":"before"}}},
            "to":"deck","position":"bottom","order":"chooser"}}
    ])
}

fn remainder_steps() -> Value {
    json!([
        {"look":{"bind":"seen","top":{"who":"self","n":5_u8}}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2_u8,"may":true}},
        {"move":{"ref":"picked","to":"hand","result":"taken"}},
        {"banish":{"ref":{"in":"seen","except":{"in":{"objects_of":"taken","view":"before"}}},"result":"banished"}}
    ])
}

#[test]
fn observed_banish_does_not_authorize_unobserved_decks_or_result_following() {
    let mut variants = Vec::new();
    let mut opponent = steps();
    opponent[0]["look"]["top"]["who"] = json!("opp");
    variants.push(opponent);
    let mut unobserved = steps();
    unobserved[1]["choose"]["from"] = json!({"who":"self","zone":"deck"});
    variants.push(unobserved);
    let mut source_result = steps();
    source_result[2]["banish"]["ref"] = json!("this");
    variants.push(source_result);
    let mut following = steps();
    following[3] = json!({"move":{"ref":{"follow":"banished"},"to":"ex"}});
    variants.push(following);
    let mut moved_input = steps();
    moved_input[3]["move"]["ref"] = json!({"in":{"objects_of":"banished","view":"before"}});
    variants.push(moved_input);
    let mut direct_foreign = remainder_steps();
    direct_foreign[0]["look"]["top"]["who"] = json!("opp");
    variants.push(direct_foreign);
    let mut direct_unobserved = remainder_steps();
    direct_unobserved[3]["banish"]["ref"] = json!({"who":"self","zone":"deck"});
    variants.push(direct_unobserved);
    let mut direct_moved = remainder_steps();
    direct_moved[3]["banish"]["ref"] = json!({"in":{"objects_of":"taken","view":"before"}});
    variants.push(direct_moved);
    for (index, body) in variants.into_iter().enumerate() {
        let (snapshot, keywords, input) = inputs(&body);
        assert!(
            Catalog::from_current_selection(&snapshot, &keywords, &input).is_err(),
            "variant {index} grants no extra private read or generation following"
        );
    }
}
