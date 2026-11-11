use super::*;

fn timed(until: &str) -> Value {
    let mut tree = input();
    tree["cards"]["unit-follower"]["abilities"][1]["activate"]["do"][0]["apply"]["until"] =
        json!(until);
    tree
}

fn decks() -> Value {
    let mut zones = json!({});
    for player in ["P1", "P2"] {
        zones[player]["deck"] = json!(
            (0_u8..6)
                .map(|n| json!({"id":format!("{player}-draw{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    zones
}

fn end_phase(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if game.projection(View::Referee).unwrap()["flow"]["stage"] == "guard" {
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
}

fn end_turn(game: &mut Game) {
    end_phase(game);
    quick::pass(game);
}

#[test]
fn next_opponent_end_waits_through_an_extra_own_turn_and_restores_each_ability_once() {
    let tree = timed("next_opp_turn");
    let mut raw = boxed::program(&tree).card().clone();
    let scope = raw.abilities[4].node.scope;
    let Node::Instruction { instruction } =
        &mut raw.abilities[4].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("extra turn fixture")
    };
    instruction.action = Action::ScheduleTurn {
        player: PlayerRef::Controller {},
        operation: TurnOperation::InsertBefore {
            anchor: ScheduleAnchor::NextTurn {},
        },
        occurrence: Binding::new(99, scope),
    };
    let card = WellFormedCardProgram::check(raw).unwrap();
    let (mut game, card) = boxed::board_with_card(&tree, false, &decks(), Some(&card));
    boxed::activate(&mut game, &card, 3);
    boxed::activate(&mut game, &card, 2);
    boxed::activate(&mut game, &card, 4);
    assert_eq!(boxed::keywords(&game), json!(["guard"]));
    boxed::activate(&mut game, &card, 5);
    for active in ["P1", "P2"] {
        end_turn(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], active);
        assert_eq!(boxed::keywords(&game), json!(["guard"]));
    }
    end_phase(&mut game);
    assert_eq!(boxed::keywords(&game), json!(["guard"]));
    quick::pass(&mut game);
    assert_eq!(boxed::keywords(&game), json!(["guard", "storm"]));
    boxed::activate(&mut game, &card, 6);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("restored Lastwords")
    };
    assert_eq!(
        options.len(),
        2,
        "one printed and one earlier granted Lastword"
    );
    let first = options[0].handle;
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(first))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("second restored Lastword")
    };
    assert_eq!(options.len(), 1, "expiry must not duplicate either ability");
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 21);
    assert_eq!(after["P2"]["hand"].as_array().unwrap().len(), 2);
}

#[test]
fn self_loss_keeps_the_current_resolution_and_later_grants_until_its_selected_turn_end() {
    for until in ["turn", "next_opp_turn", "game"] {
        let mut tree = timed(until);
        tree["cards"]["unit-follower"]["abilities"][1]["activate"]["do"] = json!([
            {"apply":{"on":"this","effect":{"lose_abilities":{}},"until":until}},
            {"grant":{"on":"this","keywords":["storm"]}},
            {"life":{"who":"self","delta":1}}
        ]);
        let (mut game, card) = boxed::board_in_zones(&tree, false, &decks());
        boxed::activate(&mut game, &card, 2);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 21,
            "self loss does not cancel this resolution"
        );
        assert_eq!(view["objects"]["s"]["keywords"], json!(["storm"]));
        let ability = card
            .card()
            .abilities
            .iter()
            .find(|a| a.source.line == 2)
            .unwrap()
            .node
            .id;
        game.activate_internal_card("s", ability, PlayerId { seat: 1 })
            .unwrap_err();
        end_turn(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            if until == "turn" {
                json!(["guard", "storm"])
            } else {
                json!(["storm"])
            }
        );
        end_turn(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            if until == "game" {
                json!(["storm"])
            } else {
                json!(["guard", "storm"])
            }
        );
    }
}

#[test]
fn raw_single_selected_operand_and_next_controller_end_use_the_same_admission() {
    let tree = timed("next_opp_turn");
    let mut raw = boxed::program(&tree).card().clone();
    let Action::RemoveAbility {
        subjects, duration, ..
    } = loss_mut(&mut raw)
    else {
        panic!("loss")
    };
    let Selector::Bound { reference } = subjects else {
        panic!("target")
    };
    *subjects = Selector::Objects {
        objects: vec![ObjectOperand::Selected {
            selection: *reference,
        }],
    };
    duration.end = DurationEnd::NextTurnEnd {
        player: PlayerRef::Controller {},
    };
    let card = WellFormedCardProgram::check(raw).unwrap();
    let (mut game, card) = boxed::board_with_card(&tree, false, &decks(), Some(&card));
    boxed::activate(&mut game, &card, 2);
    for active in ["P2", "P1"] {
        end_turn(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            active
        );
        assert_eq!(boxed::keywords(&game), json!([]));
    }
    end_turn(&mut game);
    assert_eq!(boxed::keywords(&game), json!(["guard"]));
}
