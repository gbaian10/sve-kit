#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected question invalidates the synthetic search scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

/// Followers named by their printed original cost.
const FOLLOWERS: [(&str, &str); 3] = [("one", "1"), ("two", "2"), ("three", "3")];

fn catalog() -> Arc<Catalog> {
    let mut lines = vec![json!({"number":"unit-spell","faces":[{"name":"unit-spell","card_class":"ニュートラル","card_type":"スペル","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]}).to_string()];
    let mut cards = serde_json::Map::new();
    cards.insert(
        "unit-spell".into(),
        json!({"status":"complete","review":"synthetic","abilities":[]}),
    );
    for (number, cost) in FOLLOWERS {
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":cost,"power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string());
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"synthetic","abilities":[]}),
        );
    }
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    Arc::new(
        Catalog::from_documents(
            &lines.join("\n"),
            &registry,
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn spell(from: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["spell"] = json!({
        "targets":[{"bind":"chosen","from":from,"n":1}],
        "do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower",
            "orig_cost_eq":{"plus":[{"orig_cost_of":"chosen"},1]}},"to":"hand","n":1}}]
    });
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .map(|mut cards| cards.remove("unit-spell").unwrap())
}

#[test]
fn search_reads_the_original_cost_of_the_chosen_target() {
    let program = spell(&json!({"who":"opp","zone":"field","kind":"follower"})).unwrap();
    let mut initial = setup(
        1,
        json!([
            {"id":"d1","card":"one"},{"id":"d2","card":"two"},{"id":"d3","card":"three"}
        ]),
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"two"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dynamic-cost",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    for (number, _) in FOLLOWERS {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let mut step = Some(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let mut offered = Vec::new();
    while let Some(CardPlay::Awaiting(q)) = step.take() {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(vec![d.members[0].handle]),
            InputDomain::Search(d) => {
                offered.extend(d.cards.iter().map(|card| card.card.clone()));
                Answer::Search(SearchAnswer::Ungrouped(
                    d.cards.iter().map(|card| card.handle).collect(),
                ))
            }
            _ => panic!("unexpected question: {:?}", q.domain),
        };
        step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
    }
    assert_eq!(
        offered,
        ["three"],
        "the enemy costs two, so only cost three matches"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d3"])
    );
}

#[test]
fn only_a_single_field_or_ex_target_lends_its_original_cost() {
    spell(&json!({"who":"self","zone":"cemetery","kind":"follower"})).unwrap_err();
}
