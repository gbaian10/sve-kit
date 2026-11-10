#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the full printed hand banish payment scenarios."
)]
use super::*;

#[derive(Default)]
struct Seen {
    materials: usize,
    played: usize,
    banished: usize,
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..3)
            .map(|n| json!({"id":format!("draw{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-hand-banish-cost",
    )
    .unwrap()
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut play: CardPlay, mode: ModeId, accept_fee: bool, seen: &mut Seen) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(accept_fee),
            InputDomain::CostMaterials(domain) => {
                seen.materials = seen.materials.checked_add(1).unwrap();
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the eligible hand card is offered"
                );
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "opponents do not see private cost candidates"
                );
                let before = format!("{game:?}");
                game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![u64::MAX]))
                    .unwrap_err();
                assert!(
                    format!("{game:?}") == before,
                    "foreign handles leave the entire question and authority unchanged"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Modes { modes, .. } => {
                assert!(
                    modes.contains(&ModeId(3)),
                    "mode 3 remains legal even without a hand"
                );
                assert!(
                    modes.contains(&mode),
                    "the requested mode has legal formal targets"
                );
                if !modes.contains(&ModeId(1)) {
                    let before = format!("{game:?}");
                    game.answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)]))
                        .unwrap_err();
                    assert!(
                        format!("{game:?}") == before,
                        "Q2370: mode 1 cannot be chosen without the named target"
                    );
                }
                Answer::Modes(vec![mode])
            }
            InputDomain::Targets(domain) => {
                assert_eq!(domain.members.len(), 1, "the printed target is exact");
                Answer::Targets(vec![domain.members[0].handle])
            }
            other => panic!("unexpected printed decision {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    if let CardPlay::Resolved(result) = play {
        seen.played = seen
            .played
            .checked_add(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                    .count(),
            )
            .unwrap();
        seen.banished = seen
            .banished
            .checked_add(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact,Fact::Movement(m) if m.method==MoveMethod::Banish))
                    .count(),
            )
            .unwrap();
    }
    checks(game);
}

fn settle(game: &mut Game, mode: ModeId, accept_fee: bool, seen: &mut Seen) {
    loop {
        checks(game);
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("printed pending ability");
            };
            let play = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(game, play, mode, accept_fee, seen);
            continue;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, play, mode, accept_fee, seen);
    }
}

fn evolve(game: &mut Game, mode: ModeId, accept_fee: bool) -> Seen {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base card pays its intrinsic evolution cost"
    );
    let mut seen = Seen::default();
    settle(game, mode, accept_fee, &mut seen);
    seen
}

fn saito_position(form: &str) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-117"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

#[test]
fn saito_all_printings_pay_or_decline_mode_three_and_empty_hands_do_not_hide_it() {
    for form in ["BP18-118", "BP18-SL28", "BP18-U07"] {
        for (has_hand, accept_fee) in [(true, true), (true, false), (false, false)] {
            let mut initial = saito_position(form);
            if has_hand {
                initial["players"]["P1"]["zones"]["hand"] =
                    json!([{"id":"fuel","card":"BP01-171"}]);
            }
            let mut game = board(&initial);
            let seen = evolve(&mut game, ModeId(3), accept_fee);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(seen.materials, usize::from(accept_fee));
            assert_eq!(seen.banished, usize::from(accept_fee));
            assert_eq!(
                seen.played, 1,
                "declining an optional mode fee still plays the evolution trigger"
            );
            assert_eq!(state["P1"]["banish_count"], usize::from(accept_fee));
            assert_eq!(state["P1"]["hand_count"], usize::from(has_hand));
            assert_eq!(
                state["P1"]["deck_count"],
                if accept_fee { 2_u8 } else { 3_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["objects"]["source"]["hp"], 3_u8);
        }
    }
}

#[test]
fn saito_empty_hand_acceptance_cancels_only_the_pending_play_and_allows_decline_on_retry() {
    for form in ["BP18-118", "BP18-SL28", "BP18-U07"] {
        let mut game = board(&saito_position(form));
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        checks(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("evolution creates a pending printed ability");
        };
        let origin = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(modes) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("printed modes");
        };
        let CardPlay::Awaiting(fee) = game
            .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(3)]))
            .unwrap()
        else {
            panic!("optional mode cost");
        };
        assert!(
            matches!(fee.domain, InputDomain::OptionalCost),
            "mode cost is chosen at play time"
        );
        game.answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(true))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            origin,
            "an unaffordable fee cancels the pending play without undoing the evolution"
        );
        assert!(
            game.internal_pending_choice().unwrap().is_some(),
            "the trigger can be selected again"
        );
        let mut seen = Seen::default();
        settle(&mut game, ModeId(3), false, &mut seen);
        assert_eq!(
            seen.played, 1,
            "retrying and declining still plays the trigger once"
        );
        assert_eq!(
            seen.materials, 0,
            "declining an empty-hand fee never asks for material"
        );
        assert_eq!(
            seen.banished, 0,
            "the cancelled attempt cannot move material"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "the intrinsic evolution cost remains paid"
        );
        assert_eq!(
            state["P1"]["deck_count"], 3_u8,
            "neither cancelled nor declined payment draws"
        );
    }
}

#[test]
fn saito_all_printings_recover_the_named_spell_for_zero_pp_or_heal_and_mill_without_a_target() {
    for form in ["BP18-118", "BP18-SL28", "BP18-U07"] {
        let mut initial = saito_position(form);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"flame","card":"BP18-123"}]);
        let mut game = board(&initial);
        let mut seen = evolve(&mut game, ModeId(1), true);
        let recovered = game.projection(View::Referee).unwrap();
        assert_eq!(recovered["P1"]["ex"], json!(["flame"]));
        assert_eq!(recovered["P1"]["pp"]["current"], 9_u8);
        let spell = game
            .play_internal_card("flame", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, spell, ModeId(1), true, &mut seen);
        settle(&mut game, ModeId(1), true, &mut seen);
        let after_spell = game.projection(View::Referee).unwrap();
        assert_eq!(
            after_spell["P1"]["pp"]["current"], 9_u8,
            "the recovered cost-one spell really costs zero"
        );
        assert_eq!(after_spell["objects"]["enemy"]["hp"], 18_u8);
        assert_eq!(after_spell["P2"]["leader"]["life"], 19_u8);
        let mut fallback = board(&saito_position(form));
        let fallback_seen = evolve(&mut fallback, ModeId(2), true);
        let healed = fallback.projection(View::Referee).unwrap();
        assert_eq!(healed["P1"]["leader"]["life"], 11_u8);
        assert_eq!(healed["P1"]["cemetery"], json!(["draw0"]));
        assert_eq!(fallback_seen.banished, 0, "milling is not a banish payment");
    }
}

#[test]
fn beast_tamer_both_printings_filter_pale_moon_and_keep_ward_and_named_evolution() {
    for (base, form) in [("CP03-046", "CP03-047"), ("CP03-P29", "CP03-P30")] {
        for (has_fuel, accept_fee) in [(true, true), (true, false), (false, false)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"source","card":base},{"id":"wrong","card":"BP01-171"}]);
            if has_fuel {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"fuel","card":base}));
            }
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            let mut game = board(&initial);
            let mut seen = Seen::default();
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, ModeId(1), accept_fee, &mut seen);
            settle(&mut game, ModeId(1), accept_fee, &mut seen);
            let entered = game.projection(View::Referee).unwrap();
            assert!(
                entered["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護"))
            );
            assert_eq!(seen.materials, usize::from(accept_fee));
            assert_eq!(seen.banished, usize::from(accept_fee));
            assert_eq!(
                seen.played,
                usize::from(accept_fee),
                "refusing the head fee means the whole Fanfare was not played"
            );
            assert_eq!(
                entered["P1"]["hand_count"],
                if accept_fee {
                    3_u8
                } else if has_fuel {
                    2_u8
                } else {
                    1_u8
                }
            );
            assert_eq!(entered["P1"]["pp"]["current"], 6_u8);
            if accept_fee {
                evolve(&mut game, ModeId(1), true);
                let evolved = game.projection(View::Referee).unwrap();
                assert_eq!(
                    evolved["objects"]["source"]["name"],
                    "バーキング・マンティコア"
                );
                assert_eq!(evolved["objects"]["source"]["power"], 4_u8);
                assert_eq!(
                    evolved["objects"]["source"]["keywords"],
                    json!(["守護", "ツインドライブ"])
                );
                assert_eq!(evolved["objects"]["enemy"]["hp"], 16_u8);
                assert_eq!(evolved["P1"]["pp"]["current"], 5_u8);
            }
        }
    }
}
