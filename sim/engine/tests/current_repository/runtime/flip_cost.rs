use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question};
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::DecisionView;
use sve_engine::game::player::{Input, Viewer};

fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "flip-cost",
    )
    .unwrap()
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"fuel","card":"BP01-172"},
        {"id":"used","card":"BP01-172","face_up":true},
        {"id":"drive","card":"CSD03a-017"},
        {"id":"spell","card":"CSD01-032"},
        {"id":"advance","card":"BP10-004"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    initial
}
fn target_then_material(game: &mut Game, play: CardPlay) -> DecisionView {
    let first = question(play);
    match &first.domain {
        InputDomain::Targets(domain) => question(
            game.answer_internal_card(
                first.actor,
                first.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        ),
        InputDomain::CostMaterials(_) => first,
        other => panic!("expected targets or flip materials {other:?}"),
    }
}

#[test]
fn guild_meeting_pays_a_face_down_evolve_follower_and_resolves_its_full_spell() {
    let mut game = build(&position("BP17-116"));
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let q = target_then_material(&mut game, play);
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("flip-up material question")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "used, DrivePoint and spell resources cannot pay"
    );
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
    assert!(
        game.projection(View::P2).unwrap()["objects"]
            .get("fuel")
            .is_none()
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["fuel"]["face_up"],
        false
    );
    game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![]))
        .unwrap_err();
    game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![99]))
        .unwrap_err();
    let paid = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    resolved(finish(&mut game, paid));
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["fuel"]["face_up"], true);
    assert_eq!(
        game.projection(View::P2).unwrap()["objects"]["fuel"]["card"],
        "BP01-172"
    );
    assert_eq!(state["objects"]["fuel"]["generation"], 0_u8);
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["objects"]["drive"]["face_up"], false);
}

#[test]
fn guild_meeting_is_offered_as_quick_only_with_a_target_and_payable_flip_material() {
    for (target, resource) in [(true, true), (false, true), (true, false)] {
        let mut initial = position("BP17-116");
        initial["turn"]["active"] = json!("P2");
        if !target {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        if !resource {
            initial["players"]["P1"]["zones"]["evolve_deck"][0]["face_up"] = json!(true);
        }
        let mut game = build(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let q = game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(q.actor, PlayerId { seat: 1 });
        let InputDomain::Quick { options, .. } = &q.domain else {
            panic!("Quick window")
        };
        assert_eq!(options.len(), usize::from(target && resource));
        if target && resource {
            let play = game
                .answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
                .unwrap();
            let materials = target_then_material(&mut game, play);
            let InputDomain::CostMaterials(domain) = &materials.domain else {
                panic!("Quick flip material")
            };
            let result = game
                .answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap();
            resolved(finish(&mut game, result));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["fuel"]["face_up"],
                true
            );
        } else {
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                10_u8
            );
        }
    }
}

#[test]
fn azi_dahaka_pays_three_distinct_followers_then_can_normally_evolve() {
    let mut initial = position("BP08-053");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"a","card":"BP01-172"},{"id":"b","card":"BP01-172"},
        {"id":"c","card":"BP01-172"},{"id":"d","card":"BP01-172"},
        {"id":"form","card":"BP08-054"}
    ]);
    let mut game = build(&initial);
    let entry = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entry);
    let mut step = pending(&mut game, "BP08-053");
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.cardinality.minimum, Nat(3));
                let handles = domain
                    .members
                    .iter()
                    .take(3)
                    .map(|m| m.handle)
                    .collect::<Vec<_>>();
                game.answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::CostMaterials(vec![handles[0], handles[0], handles[1]]),
                )
                .unwrap_err();
                Answer::CostMaterials(handles)
            }
            other => panic!("unexpected Azi declaration {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(step);
    let state = game.projection(View::Referee).unwrap();
    for id in ["a", "b", "c"] {
        assert_eq!(state["objects"][id]["face_up"], true);
    }
    assert_eq!(state["objects"]["d"]["face_up"], false);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    finish_checks(&mut game);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
}

#[test]
fn kay_may_flip_a_named_face_up_resource_for_its_three_pp_discount() {
    for accept in [false, true] {
        let mut initial = position("CP03-029");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"blaster","card":"CSD03a-004","face_up":true},
            {"id":"hidden","card":"CSD03a-004"},
            {"id":"other","card":"BP01-172","face_up":true},
            {"id":"drive","card":"CSD03a-017","face_up":true}
        ]);
        let mut game = build(&initial);
        let q = question(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        assert!(matches!(q.domain, InputDomain::OptionalCost));
        let mut step = game
            .answer_internal_card(q.actor, q.token, Answer::OptionalCost(accept))
            .unwrap();
        if accept {
            let materials = question(step);
            let InputDomain::CostMaterials(domain) = &materials.domain else {
                panic!("Kay flip-down material")
            };
            assert_eq!(domain.members.len(), 1);
            step = game
                .answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap();
        }
        finish(&mut game, step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["blaster"]["face_up"], !accept);
        assert_eq!(state["objects"]["blaster"]["generation"], 0_u8);
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accept { 10_u8 } else { 7_u8 }
        );
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "突進")
        );
    }
}

#[test]
fn an_unpayable_azi_fanfare_never_turns_over_a_partial_cost() {
    for (count, target) in [(2_usize, true), (3, false)] {
        let mut initial = position("BP08-053");
        initial["players"]["P1"]["zones"]["evolve_deck"] = Value::Array(
            (0..count)
                .map(|index| json!({"id":format!("fuel-{index}"),"card":"BP01-172"}))
                .collect(),
        );
        if !target {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = build(&initial);
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entry);
        finish_checks(&mut game);
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("Azi pending")
            };
            let mut step =
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle));
            loop {
                match step {
                    Err(error) => {
                        assert!(
                            matches!(error, EngineFailure::IllegalPlay(_)),
                            "unexpected failure {error}"
                        );
                        break;
                    }
                    Ok(CardPlay::Awaiting(decision)) => {
                        let answer = match &decision.domain {
                            InputDomain::OptionalCost => Answer::OptionalCost(true),
                            InputDomain::Targets(domain) => Answer::Targets(
                                domain
                                    .members
                                    .iter()
                                    .take(1)
                                    .map(|member| member.handle)
                                    .collect(),
                            ),
                            InputDomain::CostMaterials(_) => {
                                panic!("unpayable materials must not be offered")
                            }
                            other => panic!("unexpected declaration {other:?}"),
                        };
                        step = game.answer_internal_card(decision.actor, decision.token, answer);
                    }
                    Ok(_) => break,
                }
            }
        }
        let state = game.projection(View::Referee).unwrap();
        for index in 0..count {
            assert_eq!(state["objects"][format!("fuel-{index}")]["face_up"], false);
        }
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    }
}

#[test]
fn private_flip_cost_materials_cannot_be_published_before_payment() {
    let accepted = &selection().current["BP17-116"].program;
    let mut card = accepted.card().clone();
    let parameter = &mut card.card_play_costs[0]
        .node
        .additional
        .as_mut()
        .unwrap()
        .node
        .play
        .parameters[0];
    parameter.visibility = Visibility::Public {};
    WellFormedCardProgram::check(card).unwrap_err();
}
