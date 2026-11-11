#![expect(
    clippy::panic,
    reason = "Only the accepted fixture's instruction shape may occur."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn input() -> Value {
    let mut input = boxed::tree();
    input["cards"]["unit-follower"]["abilities"][1]["activate"]["do"] = json!([
        {"apply":{"on":"enemy","effect":{"lose_abilities":{}}}}
    ]);
    input
}

fn loss_mut(card: &mut CardProgram) -> &mut Action {
    let ability = card
        .abilities
        .iter_mut()
        .find(|ability| ability.source.line == 2)
        .unwrap();
    let Node::Instruction { instruction } = &mut ability.node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("ability removal instruction")
    };
    &mut instruction.action
}

#[test]
fn ability_loss_captures_existing_grants_preserves_later_ones_and_does_not_box_or_expire() {
    let (mut game, program) = boxed::board_with(&input(), true);
    boxed::activate(&mut game, &program, 3);
    assert!(
        boxed::keywords(&game)
            .as_array()
            .unwrap()
            .contains(&json!("storm"))
    );
    boxed::activate(&mut game, &program, 2);
    assert_eq!(boxed::keywords(&game), json!([]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["boxed"],
        false
    );
    boxed::activate(&mut game, &program, 4);
    assert_eq!(boxed::keywords(&game), json!(["guard"]));
    boxed::activate(&mut game, &program, 2);
    assert_eq!(boxed::keywords(&game), json!([]));
    boxed::end_turn(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["turn"]["active"], "P2");
    assert_eq!(after["objects"]["enemy"]["boxed"], false);
    assert_eq!(after["objects"]["enemy"]["acted"], false);
    assert_eq!(boxed::keywords(&game), json!([]));
}

#[test]
fn ability_loss_preserves_traits_suppresses_lastwords_and_ends_at_a_new_generation() {
    let mut input = input();
    input["cards"]["unit-follower"]["abilities"][4]["activate"]["do"] = json!([
        {"apply":{"on":"enemy","effect":{"add_trait":{"trait":"武装"}}}}
    ]);
    let (mut game, program) = boxed::board_with(&input, false);
    boxed::activate(&mut game, &program, 3);
    boxed::activate(&mut game, &program, 5);
    boxed::activate(&mut game, &program, 2);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("武装"))
    );
    boxed::activate(&mut game, &program, 6);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let departed = game.projection(View::Referee).unwrap();
    assert_eq!(departed["P1"]["cemetery"], json!(["enemy"]));
    assert_eq!(departed["P2"]["deck_count"], 1);
    boxed::activate(&mut game, &program, 8);
    let returned = game.projection(View::Referee).unwrap();
    assert_eq!(returned["objects"]["enemy"]["generation"], 2);
    assert_eq!(returned["objects"]["enemy"]["boxed"], false);
    assert_eq!(boxed::keywords(&game), json!(["guard"]));
    assert_eq!(returned["objects"]["enemy"]["traits"], json!([]));
}

#[test]
fn an_ability_pending_before_loss_keeps_its_committed_source() {
    let mut input = input();
    input["cards"]["unit-follower"]["abilities"][6]
        .as_object_mut()
        .unwrap()
        .remove("lastword");
    input["cards"]["unit-follower"]["abilities"][6]["auto"] =
        json!({"on":{"acted":{"ref":"this"}},"do":[{"life":{"who":"self","delta":1}}]});
    input["cards"]["unit-follower"]["abilities"][1]["activate"]["do"] = json!([
        {"act":"enemy"},{"apply":{"on":"enemy","effect":{"lose_abilities":{}}}}
    ]);
    let (mut game, program) = boxed::board_with(&input, false);
    boxed::activate(&mut game, &program, 2);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("saved ability")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        21
    );
    assert_eq!(boxed::keywords(&game), json!([]));
}

#[test]
fn ability_loss_until_turn_restores_live_abilities_after_the_end_phase() {
    let mut timed = input();
    let abilities = &mut timed["cards"]["unit-follower"]["abilities"];
    abilities[4]["activate"]["do"] = abilities[1]["activate"]["do"].clone();
    abilities[1]["activate"]["do"][0]["apply"]["until"] = json!("turn");
    for overlapping_loss in [false, true] {
        let (mut game, program) = boxed::board_with(&timed, false);
        boxed::activate(&mut game, &program, 3);
        assert_eq!(boxed::keywords(&game), json!(["guard", "storm"]));
        boxed::activate(&mut game, &program, 2);
        assert_eq!(boxed::keywords(&game), json!([]));
        if overlapping_loss {
            boxed::activate(&mut game, &program, 5);
        }
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        assert_eq!(
            boxed::keywords(&game),
            json!([]),
            "the end phase begins before the turn duration expires"
        );
        quick::pass(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            "P2"
        );
        assert_eq!(
            boxed::keywords(&game),
            if overlapping_loss {
                json!([])
            } else {
                json!(["guard", "storm"])
            },
            "only the ending turn's effect expires"
        );
    }
}

#[test]
fn ability_loss_rejects_unneeded_author_shapes_and_raw_scopes_or_expiry() {
    for body in [
        json!({"apply":{"on":"this","effect":{"lose_abilities":{}}}}),
        json!({"apply":{"on":"enemy","effect":{"lose_abilities":{}},"until":"next_opp_turn"}}),
        json!({"apply":{"on":"enemy","effect":{"lose_abilities":{}},"until":"game"}}),
        json!({"apply":{"on":{"who":"opp","zone":"field","kind":"follower"},"effect":{"lose_abilities":{}}}}),
        json!({"apply":{"on":"enemy","effect":{"lose_abilities":{"on":"enemy"}}}}),
    ] {
        let mut input = input();
        input["cards"]["unit-follower"]["abilities"][1]["activate"]["do"] = json!([body]);
        lowerer().lower(&input).unwrap_err();
    }
    let (_, program) = boxed::board_with(&input(), false);
    for variant in 0_u8..3 {
        let mut raw = program.card().clone();
        let Action::RemoveAbility {
            scope,
            duration,
            subjects,
            ..
        } = loss_mut(&mut raw)
        else {
            panic!("raw removal")
        };
        match variant {
            0 => {
                *scope = RemoveScope::Matching {
                    predicate: Box::new(AbilityPredicate {
                        kind: None,
                        keyword: Some(KeywordId::Ward),
                        except: vec![],
                        holder: None,
                        origin: None,
                    }),
                }
            }
            1 => duration.end = DurationEnd::EndResolution {},
            2 => *subjects = zone(ZoneKind::Field, 99),
            _ => panic!("unknown removal fixture variant"),
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
}

#[test]
fn ability_loss_suppresses_the_printed_return_to_hand_trigger() {
    let mut input = input();
    input["cards"]["unit-follower"]["abilities"][6]
        .as_object_mut()
        .unwrap()
        .remove("lastword");
    input["cards"]["unit-follower"]["abilities"][6]["auto"] = json!({"on":{"moved":{"ref":"this","from":"field","to":"hand"}},"do":[{"life":{"who":"self","delta":1}}]});
    input["cards"]["unit-follower"]["abilities"][5]["activate"]["do"] =
        json!([{"move":{"ref":"enemy","to":"hand"}}]);
    for removed in [false, true] {
        let (mut game, program) = boxed::board_with(&input, false);
        if removed {
            boxed::activate(&mut game, &program, 2);
        }
        boxed::activate(&mut game, &program, 6);
        let pending = game.internal_pending_choice().unwrap();
        if removed {
            assert!(pending.is_none());
        } else {
            let q = pending.expect("the unsuppressed return trigger must wait");
            let InputDomain::Pending(options) = &q.domain else {
                panic!("return to hand ability")
            };
            assert_eq!(options.len(), 1);
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
        }
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!(["enemy"]));
        assert_eq!(after["P2"]["leader"]["life"], if removed { 20 } else { 21 });
        assert_eq!(after["objects"]["enemy"]["generation"], 1);
    }
}
