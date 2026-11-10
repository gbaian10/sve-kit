#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the complete printed Millteo scenarios."
)]
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial
}

fn board(initial: &Value, random: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        random,
        "millteo-current",
    )
    .unwrap()
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, accept_cost: bool, accept_evolution: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept_cost),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the one own hand card pays discard"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Targets(domain) => {
                assert!(
                    domain.members.iter().all(|member| matches!(
                        member.card.as_deref(),
                        Some("BP10-075" | "BP10-SL19")
                    )),
                    "only the printed Millteo evolution resources can be chosen"
                );
                assert_eq!(
                    domain.cardinality.minimum,
                    Nat(0),
                    "effect evolution may decline its private resource"
                );
                assert!(
                    board.internal_decision(View::P2).is_none(),
                    "only the actor sees private evolution resources"
                );
                Answer::Targets(if accept_evolution {
                    vec![domain.members[0].handle]
                } else {
                    vec![]
                })
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Millteo choice {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    checks(board);
}

fn settle(board: &mut Game, accept_cost: bool, accept_evolution: bool) {
    loop {
        checks(board);
        if let Some(q) = board.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("printed pending ability")
            };
            let play = board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(board, play, accept_cost, accept_evolution);
        } else if let Some(q) = board.internal_quick_choice().unwrap() {
            let play = board
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(board, play, accept_cost, accept_evolution);
        } else {
            return;
        }
    }
}

#[test]
fn every_millteo_base_discards_then_reveals_the_original_cost_prefix_and_randomizes_only_its_remainder()
 {
    // Each printing exercises the full public source selection and the real optional hand payment.
    for number in ["BP10-074", "BP10-SL18", "BP10-U05", "SCS01-006"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"fee","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"miss","card":"BP07-T03"},{"id":"expensive","card":"BP10-074"},
            {"id":"one","card":"BP18-005"},{"id":"spell","card":"BP01-089"},
            {"id":"two","card":"BP18-005"},{"id":"tail","card":"BP18-005"}
        ]);
        let random = json!({"shuffles":[{"player":"P1","result":["spell","expensive","miss"]}]});
        let mut game = board(&initial, &random);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, true, true);
        settle(&mut game, true, true);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["field"],
            json!(["source", "one", "two"]),
            "{number}"
        );
        assert_eq!(
            after["P1"]["cemetery"],
            json!(["fee"]),
            "the optional fee really discards"
        );
        assert_eq!(
            after["P1"]["deck"],
            json!(["tail", "spell", "expensive", "miss"])
        );
        assert_eq!(after["P1"]["pp"]["current"], 5_u8);
        let hidden = game.projection(View::P2).unwrap();
        for id in ["tail", "spell", "expensive", "miss"] {
            assert!(hidden["objects"][id].is_null());
        }
    }
}

#[test]
fn declining_the_fanfare_payment_leaves_the_hand_card_and_entire_deck_untouched() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP10-074"},{"id":"fee","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP18-005"}]);
    let mut game = board(&initial, &Value::Null);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play, false, true);
    settle(&mut game, false, true);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P1"]["hand"], json!(["fee"]));
    assert_eq!(after["P1"]["deck"], json!(["top"]));
    assert_eq!(after["P1"]["cemetery"], json!([]));
}

#[test]
fn the_own_end_phase_uses_real_effect_evolution_before_the_optional_two_pp_clear_and_set_life() {
    // Reuse one fixed-source selection while covering all base/form aliases and distinct decline paths.
    for (base, form, grave, active, accept_evolution, accept_cost, life) in [
        ("BP10-074", "BP10-075", 20_u8, "P1", true, true, 20_u8),
        ("BP10-SL18", "BP10-SL19", 20, "P1", true, true, 20),
        ("BP10-U05", "BP10-075", 20, "P1", true, true, 20),
        ("SCS01-006", "BP10-SL19", 20, "P1", true, true, 3),
        ("BP10-074", "BP10-075", 20, "P1", true, false, 20),
        ("BP10-SL18", "BP10-SL19", 20, "P1", true, false, 20),
        ("BP10-074", "BP10-075", 20, "P1", false, true, 20),
        ("BP10-074", "BP10-075", 19, "P1", true, true, 20),
        ("BP10-074", "BP10-075", 20, "P2", true, true, 20),
    ] {
        let mut initial = position();
        initial["turn"]["active"] = json!(active);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":base},{"id":"ally","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!((0..grave).map(|n| {
            json!({"id":format!("grave-{n}"),"card":if n%2==0 {"BP01-171"}else{"BP01-089"}})
        }).collect::<Vec<_>>());
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-a","card":"BP01-171"},{"id":"enemy-b","card":"BP01-171"},
            {"id":"tree","card":"BP07-T03"}
        ]);
        initial["players"]["P2"]["leader"]["life"] = json!(life);
        let mut game = board(&initial, &Value::Null);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game, accept_cost, accept_evolution);
        let after = game.projection(View::Referee).unwrap();
        let evolved = grave >= 20 && active == "P1" && accept_evolution;
        let cleared = evolved && accept_cost;
        assert_eq!(
            after["objects"]["source"]["evolved_with"],
            if evolved { json!("form") } else { Value::Null }
        );
        assert_eq!(
            after["P1"]["pp"]["current"],
            if cleared { 8_u8 } else { 10_u8 },
            "effect evolution itself pays no intrinsic PP"
        );
        assert_eq!(after["P1"]["field"], json!(["source", "ally"]));
        assert_eq!(
            after["P2"]["field"],
            if cleared {
                json!(["tree"])
            } else {
                json!(["enemy-a", "enemy-b", "tree"])
            }
        );
        assert_eq!(
            after["P2"]["leader"]["life"],
            if cleared { 6_u8 } else { life }
        );
    }
}
