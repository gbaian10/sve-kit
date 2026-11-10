#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate printed target and source scenarios."
)]
use super::*;

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay) -> usize {
    let mut targets = 0_usize;
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => {
                targets = targets.checked_add(1).unwrap();
                assert_eq!(
                    domain.members.len(),
                    1,
                    "this is implicit, not a second formal target"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    checks(board);
    targets
}

fn settle(board: &mut Game) -> usize {
    let mut targets = 0_usize;
    loop {
        checks(board);
        let Some(q) = board.internal_pending_choice().unwrap() else {
            return targets;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        targets = targets.checked_add(finish(board, play)).unwrap();
    }
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "target-self-current",
    )
    .unwrap()
}

#[test]
fn disciple_all_printings_declare_targets_before_awakened_and_draw_only_after_self_damage() {
    for number in ["BP05-063", "BP05-P29"] {
        for maximum in [6_u8, 7_u8] {
            for other in [None, Some("P1"), Some("P2")] {
                let mut initial = position();
                initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
                initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
                if let Some(player) = other {
                    initial["players"][player]["zones"]["field"] = json!([{"id":"victim","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
                }
                let mut game = board(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                let declarations = finish(&mut game, play)
                    .checked_add(settle(&mut game))
                    .unwrap();
                assert_eq!(
                    declarations,
                    usize::from(other.is_some()),
                    "the header target is chosen even below Awakened; Q724 cancels when absent"
                );
                let hit = maximum == 7 && other.is_some();
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["source"]["hp"],
                    if hit { 2_u8 } else { 3_u8 }
                );
                assert_eq!(
                    state["P1"]["hand"].as_array().unwrap().len(),
                    usize::from(hit),
                    "only the own-turn ability damage draws one card"
                );
                if other.is_some() {
                    assert_eq!(
                        state["objects"]["victim"]["hp"],
                        if hit { 19_u8 } else { 20_u8 }
                    );
                }
            }
        }
    }
}

#[test]
fn leonidas_all_printings_damage_both_once_then_create_the_complete_lastword_token() {
    for form in ["BP02-023", "PR-158"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-022"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"storm","card":"BP01-089"},{"id":"royal","card":"CP02-026"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"victim","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut game = board(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert_eq!(settle(&mut game), 1);
        let after_evolution = game.projection(View::Referee).unwrap();
        assert_eq!(after_evolution["objects"]["source"]["hp"], 2_u8);
        assert_eq!(after_evolution["objects"]["victim"]["hp"], 15_u8);
        assert_eq!(after_evolution["P1"]["pp"]["current"], 9_u8);
        let storm_play = game
            .play_internal_card("storm", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, storm_play);
        settle(&mut game);
        let after_death = game.projection(View::Referee).unwrap();
        let field = after_death["P1"]["field"].as_array().unwrap();
        assert_eq!(
            field.len(),
            1,
            "the Lastword creates one token after Leonidas dies"
        );
        let token = field[0].as_str().unwrap();
        assert_eq!(after_death["objects"][token]["card"], "BP02-T03");
        assert!(
            after_death["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(after_death["P1"]["evolve_deck"], json!(["form"]));
        let royal_play = game
            .play_internal_card("royal", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, royal_play);
        settle(&mut game);
        let after_entry = game.projection(View::Referee).unwrap();
        assert_eq!(after_entry["objects"]["royal"]["power"], 4_u8);
        assert_eq!(after_entry["objects"]["royal"]["hp"], 4_u8);
        assert!(
            after_entry["objects"]["royal"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the created token retains its complete Royal entry observer"
        );
    }
}

#[test]
fn karen_both_printings_die_after_the_shared_damage_and_do_nothing_without_a_target() {
    for (base, form) in [("CP02-026", "CP02-027"), ("CP02-P20", "CP02-P21")] {
        for has_target in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            if has_target {
                initial["players"]["P2"]["zones"]["field"] =
                    json!([{"id":"victim","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
            }
            let mut game = board(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            assert_eq!(settle(&mut game), usize::from(has_target));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["field"].as_array().unwrap().len(),
                usize::from(!has_target),
                "Q927: no legal target means no damage to Karen"
            );
            if has_target {
                assert_eq!(
                    state["objects"]["victim"]["hp"], 17_u8,
                    "the opponent takes all three damage even though Karen dies in the same instruction"
                );
            } else {
                assert_eq!(state["objects"]["source"]["hp"], 2_u8);
            }
        }
    }
}

#[test]
fn disciple_does_not_draw_for_ability_damage_during_the_opponents_turn() {
    for number in ["BP05-063", "BP05-P29"] {
        let mut initial = position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"storm","card":"BP01-089"}]);
        let mut game = board(&initial);
        let play = game
            .play_internal_card("storm", PlayerId { seat: 2 })
            .unwrap();
        finish(&mut game, play);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    }
}
