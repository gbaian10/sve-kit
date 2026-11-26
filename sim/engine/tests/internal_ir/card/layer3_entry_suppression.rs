use super::*;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn input() -> Value {
    let mut tree = evolution_linked_movement::tree(&[
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"search":{
                "groups":[
                    {"sel":{"who":"self","zone":"deck","kind":"follower","orig_cost_le":3},"n":1},
                    {"sel":{"who":"self","zone":"deck","kind":"follower","orig_cost_eq":1},"n":1}
                ],"to":"field","entry_effects":[{"no_trigger":{"on":"entering","kinds":["fanfare"]}}]
            }}]}),
        ),
        (
            "auto",
            json!({"on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},"do":[{"life":{"who":"self","delta":2}}]}),
        ),
    ]);
    let source = tree["cards"]["unit-follower"].take();
    tree["cards"] = json!({"unit-amulet":source});
    let incoming = evolution_linked_movement::tree(&[(
        "fanfare",
        json!({"do":[{"life":{"who":"self","delta":3}}]}),
    )]);
    tree["cards"]["unit-follower"] = incoming["cards"]["unit-follower"].clone();
    tree
}

fn settle(game: &mut Game) {
    for _ in 0..10 {
        event_ability::finish(game);
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending entry");
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

fn board(free: u8) -> (Game, AbilityId) {
    let programs = lowerer().lower(&input()).unwrap();
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let catalog = Arc::new(
        Catalog::from_fixture_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)])
            .unwrap(),
    );
    let mut initial = setup(
        1,
        json!([
            {"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["leader"]["life"] = json!(10);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"later","card":"unit-follower"}]);
    for n in 0..4_u8.checked_sub(free).unwrap() {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("filler-{n}"),"card":"unit-follower"}));
    }
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "entry-suppression",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    let ability = programs["unit-amulet"].card().abilities[0].node.id;
    (game, ability)
}

fn search(game: &mut Game, ability: AbilityId) -> CardPlay {
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("grouped search");
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("search domain");
    };
    game.answer_internal_card(
        q.actor,
        q.token,
        Answer::Search(SearchAnswer::Grouped(vec![
            vec![domain.cards[0].handle],
            vec![domain.cards[1].handle],
        ])),
    )
    .unwrap()
}

#[test]
fn grouped_search_suppresses_only_the_found_cards_fanfare_and_preserves_other_entry_observers() {
    let (mut game, ability) = board(4);
    resolved(search(&mut game, ability));

    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        14
    );
    assert!(matches!(
        game.play_internal_card("later", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        19
    );
}

#[test]
fn suppression_follows_only_successful_entries_through_partial_or_full_capacity() {
    for free in [0, 1] {
        let (mut game, ability) = board(free);
        let result = search(&mut game, ability);
        let result = if free == 1 {
            let CardPlay::Awaiting(q) = result else {
                panic!("capacity choice");
            };
            let InputDomain::SearchPartition(domain) = &q.domain else {
                panic!("capacity domain");
            };
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::SearchPartition(vec![domain.cards[1].handle]),
            )
            .unwrap()
        } else {
            result
        };
        let result = resolved(result);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
                .count(),
            1
        );
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 10 + 2 * free);
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            2_usize.checked_sub(usize::from(free)).unwrap()
        );
    }
}

#[test]
fn entry_guards_reject_other_kinds_references_destinations_and_raw_installed_effects() {
    let base = "/cards/unit-amulet/abilities/0/activate/do/0/search";
    for (suffix, value) in [
        ("/entry_effects/0/no_trigger/on", json!("this")),
        ("/entry_effects/0/no_trigger/kinds", json!(["lastword"])),
        ("/entry_effects/0/no_trigger/kinds", json!([])),
        (
            "/entry_effects/0/no_trigger/kinds",
            json!(["fanfare", "fanfare"]),
        ),
        ("/to", json!("hand")),
    ] {
        let mut candidate = input();
        *candidate.pointer_mut(&format!("{base}{suffix}")).unwrap() = value;
        lowerer().lower(&candidate).unwrap_err();
    }
    let program = lowerer()
        .lower(&input())
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut raw = program.card().clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("search");
    };
    let Action::Search {
        destination: Some(SearchDestination::Single { movement }),
        ..
    } = &mut instruction.action
    else {
        panic!("search movement");
    };
    movement.to = MoveDestination::Owners {
        zone: OwnerZone::Hand,
    };
    WellFormedCardProgram::check(raw).unwrap_err();
}

fn move_input() -> Value {
    let mut value = input();
    value["cards"]["unit-amulet"]["abilities"][0]["activate"] = json!({
        "cost":{"pp":0},
        "targets":[{"bind":"incoming","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],
        "do":[
            {"move":{"ref":"incoming","to":"field","result":"entered",
                "entry_effects":[{"no_trigger":{"on":"entering","kinds":["fanfare"]}}]}}
        ]
    });
    let mut return_entry = value["cards"]["unit-amulet"]["abilities"][0].clone();
    return_entry["line"] = json!(3);
    return_entry["activate"] = json!({"cost":{"pp":0},
        "targets":[{"bind":"returning","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"move":{"ref":"returning","to":"hand"}}]});
    value["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(return_entry);
    value
}

#[test]
fn a_move_suppresses_only_that_entry_and_a_new_generation_triggers_normally() {
    let programs = lowerer().lower(&move_input()).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"incoming","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "move-suppression",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    let ability = programs["unit-amulet"].card().abilities[0].node.id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("entry target");
    };
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    settle(&mut game);
    let return_ability = programs["unit-amulet"].card().abilities[2].node.id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", return_ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("return target");
    };
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    settle(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 12);
    assert_eq!(before["P1"]["hand"], json!(["incoming"]));
    assert!(matches!(
        game.play_internal_card("incoming", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    settle(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 17);
    assert_ne!(
        after["objects"]["incoming"]["generation"],
        before["objects"]["incoming"]["generation"]
    );
}

#[test]
fn declining_both_search_groups_still_shuffles_without_creating_entry_triggers() {
    let (mut game, ability) = board(4);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("grouped search");
    };
    let result = resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![vec![], vec![]])),
        )
        .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
            .count(),
        1
    );
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 10);
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 2);
}
