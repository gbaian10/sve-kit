use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::InputDomain;

fn tree(condition: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    let mut header = seed["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("activate");
    header["line"] = json!(2);
    header["play"] = json!({"cannot_if":condition});
    seed["abilities"] = json!([header]);
    tree["cards"] = json!({"unit-spell":seed});
    tree
}

fn restriction(condition: &Value) -> Located<Ability> {
    let mut expanded = tree(condition);
    let mut spell = expanded["cards"]["unit-spell"]["abilities"][0].clone();
    spell["play"] = json!({"cannot_if":{"my_turn":true}});
    expanded["cards"]["unit-spell"]["abilities"]
        .as_array_mut()
        .unwrap()
        .insert(0, spell);
    let lowered = lowerer().lower(&expanded).unwrap();
    lowered["unit-spell"].card().abilities[1].clone()
}

#[test]
fn played_name_restriction_checks_prior_commits_and_keeps_names_after_ability_loss() {
    let condition = json!({"history":{"event":"played","who":"self","window":"this_turn",
        "sel":{"name":"unit-spell"},"at_least":1}});
    let spell = card(vec![play::spell(vec![]), restriction(&condition)]);
    let mut input = equipment_played::tree(
        &json!({"line":1,"static":{"effects":[{"also_named":{"name":"unit-spell"}}]}}),
        "unit-follower",
    );
    let loss = equipment_played::tree(
        &json!({"line":2,"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"lose_abilities":{}}}}]}}),
        "unit-follower",
    )["cards"]["unit-follower"]["abilities"][0].clone();
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(loss);
    let follower = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-follower"},
        {"id":"s","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "played-names",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &spell).unwrap();
    game.load_internal_card("unit-follower", &follower).unwrap();
    let mut ordinary = play::position(&spell, "hand", 0);
    resolved(
        ordinary
            .play_internal_card("s", PlayerId { seat: 1 })
            .unwrap(),
    );
    finish_play_timing(&mut ordinary);
    assert!(matches!(
        game.play_internal_card("first", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish_play_timing(&mut game);
    let loss = follower
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    resolved(
        game.activate_internal_card("first", loss, PlayerId { seat: 1 })
            .unwrap(),
    );
    finish_play_timing(&mut game);
    let before = format!("{game:?}");
    let error = game
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(&error, sve_engine::EngineFailure::Unsupported(reason)
        if reason == "internal D1 does not support card play holder, permission or timing"),
        "the saved name reaches the existing permission gate: {error:?}"
    );
    assert_eq!(
        format!("{game:?}"),
        before,
        "a past alias remains in history after that ability is lost"
    );
}

#[test]
fn printed_play_restrictions_check_origin_and_turn_before_any_authority_change() {
    for (condition, from_ex) in [
        (json!({"in_zone":{"ref":"this","zone":"ex"}}), true),
        (json!({"my_turn":true}), false),
    ] {
        let program = card(vec![play::spell(vec![]), restriction(&condition)]);
        for zone in ["hand", "ex"] {
            let mut game = play::position(&program, zone, 0);
            let before = format!("{game:?}");
            let result = game.play_internal_card("s", PlayerId { seat: 1 });
            if !from_ex || zone == "ex" {
                result.unwrap_err();
                assert_eq!(format!("{game:?}"), before);
            } else {
                resolved(result.unwrap());
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["pp"]["current"], 3);
                assert_eq!(view["P1"]["cemetery"], json!(["s"]));
            }
        }
    }
}

#[test]
fn printed_play_restrictions_filter_quick_options_but_allow_opponent_turn() {
    for (condition, forbidden_ex) in [
        (json!({"in_zone":{"ref":"this","zone":"ex"}}), true),
        (json!({"my_turn":true}), false),
    ] {
        let spell = card(vec![
            play::spell(vec![]),
            quick::permit(),
            restriction(&condition),
        ]);
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["pp"] = json!({"current":4,"max":4});
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"h","card":"unit-spell"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "restrictions",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &spell).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick");
        };
        assert_eq!(options.len(), if forbidden_ex { 1 } else { 2 });
        let hand = options.iter().find(|option| option.zone == "hand").unwrap();
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(hand.handle)),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["pp"]["current"], 3);
        assert_eq!(view["P2"]["cemetery"], json!(["h"]));
    }
}

#[test]
fn printed_play_restrictions_reject_other_conditions_and_forged_permissions_atomically() {
    for condition in [
        json!({"opp_turn":true}),
        json!({"in_zone":{"ref":"this","zone":"evolve_deck"}}),
        json!({"in_zone":{"ref":"target","zone":"ex"}}),
        json!({"all":[{"my_turn":false}]}),
    ] {
        lowerer().lower(&tree(&condition)).unwrap_err();
    }
    let valid = restriction(&json!({"my_turn":true}));
    for variant in 0..5 {
        let mut forged = valid.clone();
        match variant {
            0 => forged.node.active_zones = vec![ZoneKind::Field],
            1 => forged.node.source.controller = PlayerRef::Active {},
            2 => {
                forged.node.source.holder = ObjectOperand::Bound {
                    binding: Binding::new(1, ScopeId(3)),
                }
            }
            3 => {
                let Effect::Permission { polarity, .. } = &mut forged.node.declarations[0] else {
                    panic!("permission");
                };
                *polarity = Polarity::Allow;
            }
            _ => {
                let Effect::Permission { rule, .. } = &mut forged.node.declarations[0] else {
                    panic!("permission");
                };
                *rule = RuleId(1232);
            }
        }
        let program = CardProgram {
            abilities: vec![play::spell(vec![]), forged],
            card_play_costs: vec![],
        };
        if let Ok(checked) = WellFormedCardProgram::check(program) {
            let mut game = play::position(&card(vec![play::spell(vec![])]), "hand", 0);
            let before = format!("{game:?}");
            game.load_internal_card("unit-spell", &checked).unwrap_err();
            assert_eq!(format!("{game:?}"), before);
        }
    }
}

#[test]
fn printed_play_restrictions_evaluate_nested_public_conditions_and_reject_unknown_reads() {
    let condition = json!({"all":[{"not":{"awakened":true}}, {"cmp":{"left":{"count":{"who":"self","zone":"field","kind":"follower"}},"op":"le","right":2}}]});
    let program = card(vec![play::spell(vec![]), restriction(&condition)]);
    let mut game = play::position(&program, "hand", 0);
    let before = format!("{game:?}");
    game.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{game:?}"), before);
    for condition in [
        json!({"cmp":{"left":{"count":{"who":"self","zone":"hand","class":"ロイヤル"}},"op":"le","right":2}}),
        json!({"did":"later-move"}),
    ] {
        lowerer().lower(&tree(&condition)).unwrap_err();
    }
    let mut forged = restriction(&json!({"awakened":true}));
    let Effect::Permission { condition, .. } = &mut forged.node.declarations[0] else {
        panic!("permission");
    };
    *condition = Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Bound(Binding::new(999, forged.node.scope)),
        comparison: Comparison::Equal,
        right: Expr::Literal(Int(1)),
    }));
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![play::spell(vec![]), forged],
        card_play_costs: vec![],
    })
    .unwrap_err();
}

#[test]
fn conditional_cemetery_play_permission_lowers_to_a_source_allow_rule() {
    let mut expanded = tree(&json!({"my_turn":true}));
    expanded["cards"]["unit-spell"]["abilities"][0]["play"] = json!({
        "playable_from": {
            "zone":"cemetery",
            "if":{"cmp":{
                "left":{"history_count":{"event":"played","window":"this_turn","who":"self"}},
                "op":"ge", "right":3_u8
            }}
        }
    });
    let programs = lowerer().lower(&expanded).unwrap();
    let ability = &programs["unit-spell"].card().abilities[0].node;
    assert_eq!(ability.active_zones, vec![ZoneKind::Cemetery]);
    let [Effect::Permission { polarity, .. }] = ability.declarations.as_slice() else {
        panic!("one native play permission")
    };
    assert_eq!(*polarity, Polarity::Allow);
}

fn cemetery_game(zone: &str, permitted: bool) -> Game {
    let mut expanded = tree(&json!({"my_turn":true}));
    let mut follower = expanded["cards"]["unit-spell"].take();
    follower["abilities"][0]["play"] = json!({"playable_from":{
        "zone":"cemetery", "if":{"cmp":{
            "left":{"history_count":{"event":"played","window":"this_turn","who":"self"}},
            "op":"ge", "right":3_u8
        }}
    }});
    expanded["cards"] = json!({"unit-follower":follower});
    let program = if permitted {
        lowerer().lower(&expanded).unwrap()["unit-follower"].clone()
    } else {
        card(vec![])
    };
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-spell"},
        {"id":"second","card":"unit-spell"},
        {"id":"third","card":"unit-spell"}
    ]);
    if zone == "hand" {
        initial["players"]["P1"]["zones"][zone]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"may","card":"unit-follower"}));
    } else {
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"may","card":"unit-follower"}]);
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cemetery-permission",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
        .unwrap();
    game
}

fn finish_play_timing(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn cemetery_permission_requires_three_committed_plays_without_counting_itself() {
    let mut game = cemetery_game("cemetery", true);
    let untouched = format!("{game:?}");
    game.play_internal_card("may", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{game:?}"), untouched);
    for holder in ["first", "second", "third"] {
        resolved(
            game.play_internal_card(holder, PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_play_timing(&mut game);
        if holder != "third" {
            let after_partial = format!("{game:?}");
            game.play_internal_card("may", PlayerId { seat: 1 })
                .unwrap_err();
            assert_eq!(format!("{game:?}"), after_partial);
        }
    }
    assert!(matches!(
        game.play_internal_card("may", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish_play_timing(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["may"]));
    assert_eq!(state["P1"]["cemetery"], json!(["first", "second", "third"]));
}

#[test]
fn conditional_cemetery_permission_preserves_hand_and_ex_but_does_not_grant_other_cards() {
    for zone in ["hand", "ex"] {
        let mut game = cemetery_game(zone, true);
        assert!(matches!(
            game.play_internal_card("may", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_play_timing(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["may"])
        );
    }
    let mut unpermitted = cemetery_game("cemetery", false);
    for holder in ["first", "second", "third"] {
        resolved(
            unpermitted
                .play_internal_card(holder, PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_play_timing(&mut unpermitted);
    }
    let before = format!("{unpermitted:?}");
    unpermitted
        .play_internal_card("may", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{unpermitted:?}"), before);
}

#[test]
fn cemetery_permission_rejects_unneeded_origins_unconditional_and_mixed_play_modifiers() {
    for permission in [
        json!("cemetery"),
        json!({"zone":["cemetery"],"if":{"my_turn":true}}),
        json!({"zone":"ex","if":{"my_turn":true}}),
        json!({"zone":"hand","if":{"my_turn":true}}),
        json!({"zone":"deck","if":{"my_turn":true}}),
    ] {
        let mut expanded = tree(&json!({"my_turn":true}));
        expanded["cards"]["unit-spell"]["abilities"][0]["play"] =
            json!({"playable_from":permission});
        lowerer().lower(&expanded).unwrap_err();
    }
    for extra in [
        json!({"cannot_if":{"my_turn":true}}),
        json!({"cost_delta":-1_i8}),
        json!({"define":{"x":1_u8}}),
    ] {
        let mut expanded = tree(&json!({"my_turn":true}));
        let mut play = json!({"playable_from":{"zone":"cemetery","if":{"my_turn":true}}});
        play.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        expanded["cards"]["unit-spell"]["abilities"][0]["play"] = play;
        lowerer().lower(&expanded).unwrap_err();
    }
}

#[test]
fn granted_cost_modifiers_cannot_silently_add_a_cemetery_play_permission() {
    let mut expanded = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"trait":"財宝"},"n":1_u8},
            "do":[{"grant":{"on":"this","abilities":[{"play":{"cost_delta":-1_i8}}]}}]
        }),
    )]);
    lowerer().lower(&expanded).unwrap();
    expanded["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["grant"]["abilities"]
        [0]["play"]["playable_from"] = json!({"zone":"cemetery","if":{"my_turn":true}});
    lowerer().lower(&expanded).unwrap_err();
}

#[test]
fn hand_only_restriction_allows_hand_and_rejects_ex_without_spending() {
    let program = card(vec![
        play::spell(vec![]),
        restriction(&json!({"not":{"in_zone":{"ref":"this","zone":"hand"}}})),
    ]);
    for zone in ["hand", "ex"] {
        let mut game = play::position(&program, zone, 0);
        let before = game.projection(View::Referee).unwrap();
        let result = game.play_internal_card("s", PlayerId { seat: 1 });
        if zone == "hand" {
            resolved(result.unwrap());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!(["s"])
            );
        } else {
            assert!(matches!(
                result,
                Err(EngineFailure::Unsupported(message)) if message == "internal D1 does not support card play holder, permission or timing"
            ));
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
    }
}

#[test]
fn source_zone_restriction_is_checked_even_with_a_banish_play_permission() {
    for only_hand in [true, false] {
        let granting=lowerer().lower(&equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[
        {"player_effect":{"who":"self","until":"turn","effect":{"play_permission":{"from":["banish"],"on":{"who":"self","zone":"banish"}}}}}
    ]}}),"unit-follower")).unwrap().remove("unit-follower").unwrap();
        let mut abilities = vec![play::spell(vec![])];
        if only_hand {
            abilities.push(restriction(
                &json!({"not":{"in_zone":{"ref":"this","zone":"hand"}}}),
            ));
        }
        let program = card(abilities);
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"grant","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"s","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "hand-only-banish",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &granting).unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        resolved(
            game.activate_internal_card(
                "grant",
                granting.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let before = game.projection(View::Referee).unwrap();
        let result = game.play_internal_card("s", PlayerId { seat: 1 });
        if only_hand {
            assert!(
                matches!(&result, Err(EngineFailure::Unsupported(message)) if message == "internal D1 does not support card play holder, permission or timing"),
                "{result:?}"
            );
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            resolved(result.unwrap());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!(["s"])
            );
        }
    }
}
