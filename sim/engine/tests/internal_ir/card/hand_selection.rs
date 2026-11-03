use super::*;
use sve_engine::game::internal::InputDomain;

fn program(steps: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({"cost":{"pp":1},"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn own_hand_selection_filters_private_candidates_and_moves_only_the_chosen_card() {
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let other_view = if seat == 1 { View::P2 } else { View::P1 };
        for operation in ["banish", "bottom", "ex", "reveal"] {
            let action = match operation {
                "banish" => json!({"banish":{"ref":"selected"}}),
                "bottom" => json!({"move":{"ref":"selected","to":"deck","position":"bottom"}}),
                "ex" => json!({"move":{"ref":"selected","to":"ex"}}),
                _ => json!({"reveal":{"ref":"selected","bind":"shown"}}),
            };
            let program = program(&json!([
                {"choose":{"bind":"selected","from":{"who":"self","zone":"hand","kind":"spell","orig_cost_eq":1},"n":1}},
                action
            ]));
            let mut initial = setup(seat, json!([]));
            initial["turn"]["active"] = json!(actor);
            initial["players"][actor]["pp"] = json!({"current":5,"max":5});
            initial["players"][actor]["zones"]["field"] =
                json!([{"id":"s","card":"unit-follower"}]);
            initial["players"][actor]["zones"]["hand"] = json!([
                {"id":"h","card":"unit-spell"}, {"id":"wrong","card":"unit-follower"}
            ]);
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "hand-selection",
            )
            .unwrap();
            game.load_internal_card("unit-follower", &program).unwrap();
            game.load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", AbilityId(1), PlayerId { seat })
                .unwrap()
            else {
                panic!("private hand choice");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("hand candidates");
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("unit-spell"));
            assert!(game.internal_decision(other_view).is_none());
            assert!(game.projection(other_view).unwrap()["objects"]["h"].is_null());
            let pending = exact_authority(&game);
            for answer in [vec![], vec![0, 0], vec![99]] {
                game.answer_internal_card(question.actor, question.token, Answer::Targets(answer))
                    .unwrap_err();
                assert_eq!(exact_authority(&game), pending);
            }
            game.answer_internal_card(
                PlayerId {
                    seat: if seat == 1 { 2 } else { 1 },
                },
                question.token,
                Answer::Targets(vec![0]),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), pending);
            let result = resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            assert!(
                !result
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Discard(_)))
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view[actor]["hand"],
                if operation == "reveal" {
                    json!(["h", "wrong"])
                } else {
                    json!(["wrong"])
                }
            );
            if operation != "reveal" {
                let zone = match operation {
                    "banish" => "banish",
                    "bottom" => "deck",
                    _ => "ex",
                };
                assert_eq!(view[actor][zone], json!(["h"]));
            }
            if matches!(operation, "bottom" | "reveal") {
                assert!(game.projection(other_view).unwrap()["objects"]["h"].is_null());
            }
        }
    }
}

#[test]
fn hand_selection_is_not_a_formal_target_or_a_choice_from_the_opponents_hand() {
    for query in [
        json!({"who":"opp","zone":"hand"}),
        json!({"who":"all","zone":"hand"}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({"do":[
            {"choose":{"bind":"selected","from":query,"n":1}},
            {"banish":{"ref":"selected"}}
        ]});
        lowerer().lower(&tree).unwrap_err();
    }
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({
        "targets":[{"bind":"selected","from":{"who":"self","zone":"hand"},"n":1}],
        "do":[{"banish":{"ref":"selected"}}]
    });
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn a_selected_hand_follower_enters_through_common_capacity_rules() {
    for full in [false, true] {
        let mut initial = setup(1, json!([]));
        let mut field = vec![json!({"id":"s","card":"unit-follower"})];
        if full {
            field.extend((0..4).map(|n| json!({"id":format!("f{n}"),"card":"unit-follower"})));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "hand-entry",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program(&json!([
            {"choose":{"bind":"selected","from":{"who":"self","zone":"hand","kind":"follower"},"n":1}},
            {"move":{"ref":"selected","to":"field","result":"entered"}}
        ]))).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("hand");
        };
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if full { json!(["h"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("h")),
            !full
        );
        assert_eq!(state["P1"]["pp"]["current"], 1);
    }
}

#[test]
fn empty_hand_choice_keeps_the_private_question_and_allows_the_available_zero() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{ "id":"s", "card":"unit-follower" }]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "empty-hand",
    )
    .unwrap();
    game.load_internal_card(
        "unit-follower",
        &program(&json!([
            {"choose":{"bind":"selected","from":{"who":"self","zone":"hand"},"n":1}},
            {"banish":{"ref":"selected"}}
        ])),
    )
    .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("empty choice");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("empty hand");
    };
    assert_eq!(domain.members.len(), 0);
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
            .unwrap(),
    );
}

#[test]
fn old_hand_selections_do_not_follow_a_new_generation_and_revealed_deck_moves_conceal_location() {
    for action in [
        json!({"banish":{"ref":"selected"}}),
        json!({"reveal":{"ref":"selected","bind":"shown"}}),
        json!({"move":{"ref":"selected","to":"deck","position":"bottom"}}),
    ] {
        let to_deck = action.get("move").is_some();
        let banish = action.get("banish").is_some();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([{ "id":"s", "card":"unit-follower" }]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":"unit-spell"}]);
        let first = if to_deck {
            json!({"reveal":{"ref":"selected","bind":"shown"}})
        } else if banish {
            json!({"move":{"ref":"selected","to":"deck","position":"bottom"}})
        } else {
            json!({"move":{"ref":"selected","to":"ex"}})
        };
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "hand-generation",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program(&json!([
            {"choose":{"bind":"selected","from":{"who":"self","zone":"hand"},"n":1}}, first, action
        ]))).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("hand");
        };
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"][if to_deck || banish { "deck" } else { "ex" }],
            json!(["h"])
        );
        assert_eq!(state["P1"]["banish"], json!([]));
        if to_deck || banish {
            assert!(game.projection(View::P2).unwrap()["objects"]["h"].is_null());
        }
    }
}

#[test]
fn hand_bindings_reject_unadmitted_movement_destruction_and_leader_selectors() {
    for action in [
        json!({"move":{"ref":"selected","to":"hand"}}),
        json!({"move":{"ref":"selected","to":"cemetery"}}),
        json!({"destroy":{"ref":"selected"}}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({"do":[
            {"choose":{"bind":"selected","from":{"who":"self","zone":"hand"},"n":1}}, action
        ]});
        lowerer().lower(&tree).unwrap_err();
    }
    for choose in [
        json!({"bind":"selected","by":"opp","from":{"who":"self","zone":"hand"},"n":1}),
        json!({"bind":"selected","from":{"who":"self","zone":"hand","leader":"opp"},"n":1}),
        json!({"bind":"selected","from":{"who":"self","zone":"hand","leader_or":true},"n":1}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = json!({"do":[{"choose":choose}]});
        lowerer().lower(&tree).unwrap_err();
    }
}
