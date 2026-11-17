use super::*;

fn abilities() -> Vec<Value> {
    vec![
        json!({"keywords":["storm"]}),
        json!({"static":{"effects":[{"ignore_ward":{"on":"this"}}]}}),
    ]
}

fn inputs(abilities: Vec<Value>) -> (String, String, Value) {
    let (source, source_author) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        abilities,
    );
    let (guard, guard_author) = author(
        "guard",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![json!({"keywords":["ward"]})],
    );
    let (ordinary, ordinary_author) = author(
        "ordinary",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    (
        format!("{source}\n{guard}\n{ordinary}\n"),
        include_str!("../../../../authored/rules/keywords.yaml").into(),
        json!({"documents":[],"authors":[source_author,guard_author,ordinary_author]}),
    )
}

#[test]
fn ward_bypass_does_not_open_other_subjects_zones_or_installation() {
    let mut variants = Vec::new();
    for on in [json!({"who":"all","zone":"field"}), json!("ally")] {
        let mut body = abilities();
        body[1]["static"]["effects"][0]["ignore_ward"]["on"] = on;
        variants.push(body);
    }
    let mut hand = abilities();
    hand[1]["static"]["zone"] = json!("hand");
    variants.push(hand);
    let mut installed = abilities();
    installed[1] = json!({"fanfare":{"do":[{"apply":{"on":"this","effect":{"ignore_ward":{"on":"affected"}},"until":"turn"}}]}});
    variants.push(installed);
    for body in variants {
        let (snapshot, keywords, input) = inputs(body);
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
}
