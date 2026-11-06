#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these complete card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":10_u8,"hp":10_u8,"max_hp":10_u8,"acted":true}}
    ]);
    initial
}

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.iter().take(1).map(|m| m.handle).collect())
            }
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards.iter().take(1).map(|m| m.handle).collect(),
            )),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement => panic!("unexpected card decision"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
}

fn checks(board: &mut Game) {
    for _ in 0_u8..100 {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
            continue;
        }
        if let Some(q) = board.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("pending")
            };
            let step = board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            finish(board, step);
            continue;
        }
        if let Some(q) = board.internal_quick_choice().unwrap() {
            let step = board
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(board, step);
            continue;
        }
        return;
    }
    panic!("card checks did not settle");
}

fn play(board: &mut Game, id: &str) {
    let step = board.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(board, step);
    checks(board);
}

#[test]
fn rapid_fire_uses_grave_thresholds_and_the_targets_leader() {
    for (card, grave, damage, leader_life, target_hp) in [
        ("BP11-045", 0_u8, 2_u8, 20_u8, 10_u8),
        ("BP11-P12", 2, 2, 20, 10),
        ("BP11-P12", 3, 3, 20, 10),
        ("BP11-P12", 4, 3, 20, 10),
        ("BP11-045", 5, 3, 16, 3),
    ] {
        let mut initial = position();
        initial["players"]["P2"]["zones"]["field"][0]["state"]["hp"] = json!(target_hp);
        initial["players"]["P2"]["zones"]["field"][0]["state"]["max_hp"] = json!(target_hp);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((0..grave)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP11-045"}))
            .collect::<Vec<_>>());
        let mut board = game(&initial);
        play(&mut board, "spell");
        let state = board.projection(View::Referee).unwrap();
        if target_hp > damage {
            assert_eq!(state["objects"]["enemy"]["hp"], target_hp - damage);
        } else {
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        }
        assert_eq!(state["P2"]["leader"]["life"], leader_life);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(grave) + 1
        );
    }
}

#[test]
fn mizer_searches_a_spell_loots_and_discounts_only_at_spellchain_five() {
    for grave in [0_u8, 5] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mizer","card":"BP11-SL09"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((0..grave)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP11-045"}))
            .collect::<Vec<_>>());
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"fire","card":"BP11-P12"},{"id":"draw0","card":"BP01-171"},{"id":"draw1","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        play(&mut board, "mizer");
        let searched = board.projection(View::Referee).unwrap();
        assert_eq!(searched["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(searched["P1"]["ex"], json!(["fire"]));
        assert_eq!(searched["P1"]["pp"]["current"], 8_u8);
        play(&mut board, "fire");
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if grave == 5 { 8_u8 } else { 7 }
        );
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 0);
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1);
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(grave) + 2
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if grave == 5 { 16_u8 } else { 20 }
        );
    }
}

#[test]
fn onion_gains_storm_and_an_attack_trigger_only_at_five_in_its_grave() {
    for grave in [4_u8, 5] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"onion","card":"BP09-P15"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((0..grave)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP09-049"}))
            .collect::<Vec<_>>());
        let mut board = game(&initial);
        play(&mut board, "onion");
        let result = board
            .internal_rule_action(&json!({"do":"attack","attacker":"onion","target":"P2.leader"}));
        if grave == 4 {
            assert_eq!(result.unwrap().outcome, "cannot-attack");
            assert_eq!(
                board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                20_u8
            );
        } else {
            result.unwrap();
            checks(&mut board);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["leader"]["life"], 19_u8);
            assert_eq!(state["objects"]["enemy"]["hp"], 9_u8);
        }
    }
}

#[test]
fn burzents_end_phase_places_only_an_onion_and_returns_other_cards() {
    for top in ["BP09-049", "BP01-171"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"burzent","card":"BP14-039"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"top","card":top},{"id":"below","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        board
            .play_internal_card("burzent", PlayerId { seat: 1 })
            .unwrap_err();
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        checks(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["burzent"]));
        if top == "BP09-049" {
            assert_eq!(state["P1"]["field"], json!(["top"]));
            assert_eq!(state["P1"]["deck"], json!(["below"]));
        } else {
            assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 0);
            assert!(
                state["P1"]["deck"] == json!(["top", "below"])
                    || state["P1"]["deck"] == json!(["below", "top"])
            );
        }
    }
}

#[test]
fn evolved_burzent_discards_an_onion_draws_two_and_returns_to_ex_on_death() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"burzent","card":"BP14-039"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-040"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":"BP09-049"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"},{"id":"draw1","card":"BP01-171"},{"id":"spare","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"burzent","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    checks(&mut board);
    let evolved = board.projection(View::Referee).unwrap();
    assert_eq!(evolved["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(evolved["P1"]["cemetery"], json!(["payment"]));
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"burzent","target":"enemy"}))
        .unwrap();
    checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["burzent"]));
    assert_eq!(state["P1"]["cemetery"], json!(["payment"]));
}

fn construction_card(card: &str, alias: &str) {
    assert_eq!(
        selection().current[card].card_id,
        selection().current[alias].card_id,
        "all printings share one admitted identity"
    );
    assert_eq!(
        selection().current[card].program.card().abilities.len(),
        0,
        "construction text creates no game ability"
    );
    let mut initial = position();
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!((0_u8..10)
        .map(|n| json!({"id":format!("copy{n}"),"card":alias}))
        .collect::<Vec<_>>());
    let mut board = game(&initial);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["evolve_deck"].as_array().unwrap().len(),
        10,
        "ten copies remain loadable as evolve resources"
    );
    board
        .play_internal_card("copy0", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "an illegal spell play changes no game state"
    );
}

#[test]
fn ecp01_059_keeps_ten_copies_in_the_evolve_deck() {
    construction_card("ECP01-059", "ECP01-SL28");
}
#[test]
fn ecp01_060_keeps_ten_copies_in_the_evolve_deck() {
    construction_card("ECP01-060", "ECP01-SL29");
}
#[test]
fn ecp01_061_keeps_ten_copies_in_the_evolve_deck() {
    construction_card("ECP01-061", "ECP01-SL30");
}
#[test]
fn ecp01_062_keeps_ten_copies_in_the_evolve_deck() {
    construction_card("ECP01-062", "ECP01-SL31");
}
