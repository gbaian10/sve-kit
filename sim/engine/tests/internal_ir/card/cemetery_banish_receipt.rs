use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{Answer, InputDomain};
use sve_engine::ir::lowering::LoweringError;

fn tree(query: &Value, interrupt: bool) -> Value {
    let mut actions = vec![json!({"choose":{"bind":"fuel","from":query,"n":4_u8}})];
    if interrupt {
        actions.extend([
            json!({"choose":{"bind":"one","from":{"who":"self","zone":"cemetery","kind":"amulet"},"n":1_u8}}),
            json!({"move":{"ref":"one","to":"hand"}}),
        ]);
    }
    actions.extend([
        json!({"banish":{"ref":"fuel","result":"banished"}}),
        json!({"if":{"cond":{"cmp":{"left":{"count_of":"banished"},"op":"eq","right":4_u8}},"then":[{"move":{"ref":"this","to":"hand"}}]}}),
    ]);
    equipment_played::tree(
        &json!({"spell":{"do":[{"optional":{"do":actions}}]}}),
        "unit-spell",
    )
}

fn lower(tree: &Value) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
    lowerer().lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
}

fn fixture(tree: &Value) -> Game {
    let programs = lower(tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-spell"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"r0","card":"unit-spell"},{"id":"r1","card":"unit-follower"},
        {"id":"r2","card":"unit-spell"},{"id":"stale","card":"unit-amulet"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "banish-receipt",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &programs["unit-spell"])
        .unwrap();
    for number in ["unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn cemetery_banish_receipt_ignores_a_stale_selected_generation() {
    for interrupt in [false, true] {
        let mut game = fixture(&tree(&json!({"who":"self","zone":"cemetery"}), interrupt));
        let mut play = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        while let CardPlay::Awaiting(q) = play {
            let answer = if let InputDomain::Targets(domain) = &q.domain {
                assert!(
                    matches!(domain.members.len(), 1 | 4),
                    "one interruption or four fuel"
                );
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect())
            } else {
                assert!(
                    matches!(q.domain, InputDomain::Integer(_)),
                    "the optional banish decision"
                );
                Answer::Integer(Nat(1))
            };
            play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["banish"].as_array().unwrap().len(),
            if interrupt { 3 } else { 4 }
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if interrupt { json!(["s"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["hand"],
            if interrupt {
                json!(["stale"])
            } else {
                json!(["s"])
            }
        );
    }
}

#[test]
fn only_an_exact_own_cemetery_choice_can_name_this_banish_receipt() {
    for query in [
        json!({"who":"opp","zone":"cemetery"}),
        json!({"who":"self","zone":"ex"}),
        json!({"who":"self","zone":"field"}),
        json!({"who":"self","zone":["cemetery","banish"]}),
    ] {
        lower(&tree(&query, false)).unwrap_err();
    }
    let mut variable = tree(&json!({"who":"self","zone":"cemetery"}), false);
    let choice = variable["cards"]["unit-spell"]["abilities"][0]["spell"]["do"][0]["optional"]["do"][0]["choose"].as_object_mut().unwrap();
    choice.remove("n");
    choice.insert("upto".into(), json!(4_u8));
    lower(&variable).unwrap_err();

    let mut source = tree(&json!({"who":"self","zone":"cemetery"}), false);
    let body = &mut source["cards"]["unit-spell"]["abilities"][0]["spell"]["do"];
    body[0]["optional"]["do"][2] = json!({"move":{"ref":"banished","to":"hand"}});
    lower(&source).unwrap_err();

    let mut escape = tree(&json!({"who":"self","zone":"cemetery"}), false);
    let body = &mut escape["cards"]["unit-spell"]["abilities"][0]["spell"]["do"];
    let read = body[0]["optional"]["do"]
        .as_array_mut()
        .unwrap()
        .pop()
        .unwrap();
    body.as_array_mut().unwrap().push(read);
    lower(&escape).unwrap_err();
}
