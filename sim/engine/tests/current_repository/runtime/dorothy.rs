use super::event_cemetery::finish_checks;
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "dorothy",
    )
    .unwrap()
}

fn graves(witches: usize, spells: usize, others: usize) -> Value {
    json!(
        (0..witches)
            .map(|n| json!({"id":format!("witch{n}"),"card":"BP01-059"}))
            .chain((0..spells).map(|n| json!({"id":format!("spell{n}"),"card":"BP01-065"})))
            .chain((0..others).map(|n| json!({"id":format!("other{n}"),"card":"BP01-171"})))
            .collect::<Vec<_>>()
    )
}

#[test]
fn dorothy_discards_the_whole_hand_and_fills_ex_until_full_or_deck_empty() {
    for (old, deck, moved) in [(5_usize, 3_usize, 0_usize), (0, 2, 2), (2, 0, 0), (0, 7, 5)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-035"},{"id":"discarded","card":"BP01-171"},{"id":"discarded-spell","card":"BP01-065"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..old)
                .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0..deck)
                .map(|n| json!({"id":format!("top{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        assert!(
            matches!(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the six-cost follower enters before its fanfare"
        );
        finish_checks(&mut board);
        let question = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("Dorothy fanfare")
        };
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
        );
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!([]));
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["discarded", "discarded-spell"])
        );
        assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), old + moved);
        assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), deck - moved);
        assert_ne!(
            view["game"]["ended"], true,
            "Q286 empty deck movement does not draw or lose"
        );
        assert!(
            board.internal_decision(View::P1).is_none(),
            "Q285 cannot voluntarily stop the repeated placement"
        );
    }
}

#[test]
fn dorothy_both_forms_replace_resolution_spellchain_with_spells_and_witch_followers() {
    for number in ["BP02-035", "BP02-036"] {
        for (witches, spells, others, providers, zone, silenced, allowed) in [
            (10_usize, 0_usize, 0_usize, 1_usize, "field", false, true),
            (4, 6, 3, 1, "field", false, true),
            (9, 0, 3, 1, "field", false, false),
            (5, 0, 0, 2, "field", false, false),
            (10, 0, 0, 1, "field", true, false),
            (10, 0, 0, 1, "cemetery", false, false),
        ] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"probe","card":"BP01-065"}]);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"top","card":"BP01-171"},{"id":"next","card":"BP01-171"}]);
            initial["players"]["P1"]["zones"]["cemetery"] = graves(witches, spells, others);
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            for n in 0..providers {
                initial["players"]["P1"]["zones"][zone].as_array_mut().unwrap().push(json!({"id":format!("provider{n}"),"card":number,"state":{"silenced":silenced}}));
            }
            let mut board = game(&initial);
            resolved(
                board
                    .play_internal_card("probe", PlayerId { seat: 1 })
                    .unwrap(),
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if allowed { 8_u8 } else { 7 },
                "Q284 applies only with a live unsilenced field provider, and each grave member counts once"
            );
        }
    }
}

#[test]
fn dorothy_population_also_applies_to_frozen_play_time_choice_counts() {
    for (witches, owner, expected) in [(9_usize, "P1", 2_u64), (10, "P1", 3), (10, "P2", 2)] {
        let mut initial = draw_position();
        initial["players"][owner]["zones"]["field"] = json!([{"id":"provider","card":"BP02-035"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"probe","card":"BP02-045"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = graves(witches, 0, 0);
        let mut board = game(&initial);
        let CardPlay::Awaiting(question) = board
            .play_internal_card("probe", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("Choice mode declaration")
        };
        let InputDomain::Modes { count, .. } = question.domain else {
            panic!("frozen AtPlay count")
        };
        assert_eq!(
            count.0, expected,
            "the controller's effective population is captured at play time"
        );
    }
}

fn evolution_position(spell: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-035"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-036"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"follower","card":"BP01-171"},{"id":"spell","card":spell}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..8_usize)
            .map(|n| json!({"id":format!("draw{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0..4_usize)
            .map(|n| json!({"id":format!("enemy-draw{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn evolve(board: &mut Game) {
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the printed evolution costs one PP"
    );
    finish_checks(board);
    let mut triggers = 0_u8;
    while let Some(question) = board.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("independent evolution triggers")
        };
        let mut play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(targets) = play {
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("printed EX follower or spell target")
            };
            assert_eq!(
                domain.members.len(),
                1,
                "each independent evolution ability offers only its own card kind"
            );
            play = board
                .answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
        }
        resolved(play);
        triggers = triggers.checked_add(1).unwrap();
        finish_checks(board);
    }
    assert_eq!(triggers, 2, "both evolution abilities resolve separately");
}

fn next_turn(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if board.internal_quick_choice().unwrap().is_none() {
        board
            .internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
    damage_prevent::pass_quick(board);
}

#[test]
fn dorothy_evolution_discounts_follower_and_spell_independently_until_turn_end() {
    for expired in [false, true] {
        let mut board = game(&evolution_position("BP01-065"));
        evolve(&mut board);
        if expired {
            next_turn(&mut board);
            next_turn(&mut board);
        }
        let before = board.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_i64()
            .unwrap();
        assert!(
            matches!(
                board
                    .play_internal_card("follower", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the selected follower remains an ordinary proper play"
        );
        finish_checks(&mut board);
        let after_follower = board.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_i64()
            .unwrap();
        assert_eq!(
            before - after_follower,
            i64::from(expired),
            "the follower delta floors at zero and expires at turn end"
        );
        resolved(
            board
                .play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            after_follower
                - board.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                    .as_i64()
                    .unwrap(),
            if expired { 3_i64 } else { 0 },
            "the spell keeps its separate delta and the same duration"
        );
    }
}

#[test]
fn dorothy_q329_applies_the_fixed_dimensionshift_cost_before_its_minus_five_delta() {
    let mut initial = evolution_position("BP01-057");
    initial["players"]["P1"]["zones"]["cemetery"] = graves(0, 10, 0);
    let mut board = game(&initial);
    evolve(&mut board);
    let CardPlay::Awaiting(declaration) = board
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("optional fixed-cost exchange")
    };
    assert_eq!(
        declaration.domain,
        InputDomain::OptionalCost,
        "the fixed seven cost is still optional"
    );
    let CardPlay::Awaiting(payment) = board
        .answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(true),
        )
        .unwrap()
    else {
        panic!("ten actual spell materials")
    };
    let InputDomain::CostMaterials(domain) = &payment.domain else {
        panic!("grave spell payment")
    };
    assert_eq!(
        domain.members.len(),
        10,
        "the population override does not rewrite spell-only cost materials"
    );
    resolved(
        board
            .answer_internal_card(
                payment.actor,
                payment.token,
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
            )
            .unwrap(),
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "ten starting PP minus one evolution and the fixed-seven minus-five spell cost"
    );
    assert_eq!(view["P1"]["banish"].as_array().unwrap().len(), 10);
}
