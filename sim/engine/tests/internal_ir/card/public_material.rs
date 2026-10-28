use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

fn expanded(card: &str, head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({card:{"meta":meta,"abilities":[entry]}});
    tree
}

fn program(card: &str, head: &str, body: Value) -> WellFormedCardProgram {
    ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(
            &expanded(card, head, body),
            &BTreeMap::from([(card.to_owned(), Nat(1))]),
        )
        .unwrap()[card]
        .clone()
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture question fails the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}

#[test]
fn optional_act_materials_filter_class_kind_and_standing_after_targets() {
    let typed = program(
        "unit-spell",
        "spell",
        json!({
            "optional_cost":{"act":{"sel":{"who":"self","zone":"field","kind":"follower","class":"ロイヤル"},"n":1},"result":"paid"},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"damage":{"to":"enemy","n":1}}]}}]
        }),
    );
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["card_class"] = json!("ロイヤル");
            }
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー");
                row["faces"][0]["power"] = json!("2");
                row["faces"][0]["hp"] = json!("3");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"royal","card":"unit-follower"},
        {"id":"acted","card":"unit-follower","state":{"acted":true}},
        {"id":"neutral","card":"unit-amulet"}
    ]);
    for accepted in [false, true] {
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "material-class",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &typed).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let declaration = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let target = awaiting(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let play = game
            .answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap();
        let result = if accepted {
            let material = awaiting(play);
            let InputDomain::CostMaterials(domain) = &material.domain else {
                panic!("material")
            };
            assert_eq!(domain.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    material.actor,
                    material.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
            )
        } else {
            resolved(play)
        };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["royal"]["acted"], accepted);
        assert_eq!(state["objects"]["acted"]["acted"], true);
        assert_eq!(state["objects"]["neutral"]["acted"], false);
        assert_eq!(state["objects"]["f"]["damage"], u8::from(accepted));
        assert_eq!(state["P1"]["pp"]["current"], 1);
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
        );
    }
}

#[test]
fn trigger_head_requires_five_own_cemetery_cards_or_cancels_without_playing() {
    let typed = program(
        "unit-follower",
        "fanfare",
        json!({
            "cost":{"banish":{"sel":{"who":"self","zone":"cemetery"},"n":5}},
            "do":[{"life":{"who":"self","delta":1}}]
        }),
    );
    for count in [4, 5] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..count)
                .map(|n| json!({"id":format!("c{n}"),"card":"unit-spell"}))
                .collect(),
        );
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "cemetery-head",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        assert!(matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let declaration = awaiting(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
                .unwrap(),
        );
        let result = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(true),
            )
            .unwrap();
        if count == 4 {
            assert_eq!(result, CardPlay::Cancelled);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 4);
            assert_eq!(state["P1"]["leader"]["life"], 20);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        let question = awaiting(result);
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("materials")
        };
        assert_eq!(domain.members.len(), 5);
        let before = exact_authority(&game);
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1, 2, 3, 3]),
        )
        .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0, 1, 2, 3, 4]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 5);
        assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
        assert_eq!(state["P1"]["leader"]["life"], 21);
        let played = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .unwrap();
        assert_eq!(
            result.progress.facts[..played]
                .iter()
                .filter(|fact| matches!(fact, Fact::Movement(_)))
                .count(),
            5
        );
    }
}

#[test]
fn optional_public_cost_rejects_extra_operations_and_unknown_keys() {
    let valid = json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"paid"},"do":[{"draw":{"n":1}}]});
    let lowerer = ExpandedLowerer::new().unwrap();
    let costs = BTreeMap::from([("unit-spell".to_owned(), Nat(1))]);
    lowerer
        .lower_with_printed_costs(&expanded("unit-spell", "spell", valid.clone()), &costs)
        .unwrap();
    for (extra, value) in [
        (
            "to_cemetery",
            json!({"sel":{"who":"self","zone":"field"},"n":1}),
        ),
        ("unknown", json!(true)),
    ] {
        let mut body = valid.clone();
        body["optional_cost"][extra] = value;
        lowerer
            .lower_with_printed_costs(&expanded("unit-spell", "spell", body), &costs)
            .unwrap_err();
    }
}

#[test]
fn declining_one_head_keeps_the_other_pending_ability_of_the_same_holder() {
    let mut tree = expanded(
        "unit-follower",
        "fanfare",
        json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}),
    );
    let mut second = tree["cards"]["unit-follower"]["abilities"][0].clone();
    second["line"] = json!(3);
    second["sources"][0]["occurrence"]["line_ordinal"] = json!(2);
    second["fanfare"] = json!({"do":[{"life":{"who":"self","delta":2}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let typed = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "head-occurrences",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &typed).unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending")
    };
    assert_eq!(options.len(), 2);
    let head = options
        .iter()
        .find(|option| option.line == 2)
        .unwrap()
        .handle;
    let declaration = awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(head))
            .unwrap(),
    );
    assert_eq!(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(false)
        )
        .unwrap(),
        CardPlay::Cancelled
    );
    game.resume_internal_card().unwrap();
    let remaining = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &remaining.domain else {
        panic!("remaining")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 3);
    let result = resolved(
        game.answer_internal_card(
            remaining.actor,
            remaining.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1
    );
}
