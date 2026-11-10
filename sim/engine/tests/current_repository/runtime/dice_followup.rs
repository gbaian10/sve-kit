#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed dice scenarios."
)]
use super::*;

fn printings(number: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[number].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(printing, _)| printing.clone())
        .collect()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},
        {"id":"d3","card":"BP01-171"},{"id":"d4","card":"BP01-171"}
    ]);
    initial
}

fn seeded(initial: &Value, seed: &str) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        seed,
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, mode: usize, reroll: bool) -> Vec<DieFace> {
    let mut rolls = vec![];
    loop {
        while let CardPlay::Awaiting(q) = play {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    Answer::Targets(d.members.iter().take(1).map(|m| m.handle).collect())
                }
                InputDomain::CostMaterials(d) => {
                    Answer::CostMaterials(d.members.iter().take(1).map(|m| m.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[mode]]),
                InputDomain::DiceReroll { .. } => Answer::Integer(Nat(u64::from(reroll))),
                other => panic!("unexpected dice followup question: {other:?}"),
            };
            play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        if let CardPlay::Resolved(done) = &play {
            rolls.extend(done.progress.facts.iter().filter_map(|f| match f {
                Fact::Dice(event) => Some(event.value),
                _ => None,
            }));
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return rolls;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending dice observer")
        };
        play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
}

fn settle(game: &mut Game) -> Vec<DieFace> {
    let mut rolls = vec![];
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return rolls;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending ability")
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        rolls.extend(finish(game, play, 0, false));
    }
}

fn activate(game: &mut Game, holder: &str) -> Vec<DieFace> {
    let ability = game.internal_activated_sources(holder).unwrap()[0].definition;
    let play = game
        .activate_internal_card(holder, ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, play, 0, false)
}

fn evolve(game: &mut Game, super_evolution: bool, reroll: bool) -> Vec<DieFace> {
    let payment = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":payment})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the legal evolution action itself must resolve"
    );
    let mut rolls = vec![];
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return rolls;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("evolution observer")
        };
        let observer = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        rolls.extend(finish(game, observer, 0, reroll));
    }
}

#[test]
fn bp21_075_its_own_fanfare_triggers_both_independent_dice_abilities() {
    for number in printings("BP21-075") {
        for (seed, face, damage, life) in [
            ("dice-3", DieFace::Six, 6_u8, 16_u8),
            ("dice-0", DieFace::Three, 3, 20),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
            let mut game = seeded(&initial, seed);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            assert_eq!(finish(&mut game, play, 0, false), vec![face]);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["enemy"]["hp"], 10_u8 - damage);
            assert_eq!(view["P2"]["leader"]["life"], life);
        }
    }
}

#[test]
fn bp21_074_super_evolution_revives_academy_and_rolls_three_times() {
    for number in printings("BP21-074") {
        let mut initial = position();
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP21-073"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"revived","card":"BP21-087"},{"id":"wrong","card":"BP01-171"}]);
        let mut game = seeded(&initial, "card-seed");
        assert_eq!(
            evolve(&mut game, true, false),
            vec![DieFace::One, DieFace::Two, DieFace::Five]
        );
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
        assert_eq!(view["P1"]["cemetery"], json!(["wrong"]));
        assert_eq!(view["P1"]["sep"], 0_u8);
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn bp21_077_dice_event_deals_three_only_on_six() {
    for number in printings("BP21-077") {
        for (seed, hp) in [("dice-3", 7_u8), ("dice-0", 9)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":number},{"id":"roller","card":"BP21-083"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
            let mut game = seeded(&initial, seed);
            activate(&mut game, "roller");
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                hp
            );
        }
    }
}

#[test]
fn bp21_078_evolution_rolls_twice_with_separate_captured_values() {
    for number in printings("BP21-078") {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP21-077"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut game = seeded(&initial, "dice-3");
        assert_eq!(
            evolve(&mut game, false, false),
            vec![DieFace::Six, DieFace::Four]
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            6_u8
        );
    }
}

#[test]
fn bp21_087_six_buffs_one_own_follower_and_activation_has_a_turn_limit() {
    for number in printings("BP21-087") {
        for (seed, power, hp) in [("dice-3", 3_u8, 4_u8), ("dice-0", 2, 3)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            let mut game = seeded(&initial, seed);
            let ability = game.internal_activated_sources("source").unwrap()[0].definition;
            let roll = game
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap();
            let CardPlay::Resolved(_) = roll else {
                panic!("zero-cost roll completes")
            };
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("own dice observer")
            };
            let observer = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let CardPlay::Awaiting(target) = &observer else {
                panic!("choose the formal target even without a six")
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("own follower target")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].player, PlayerId { seat: 1 });
            finish(&mut game, observer, 0, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["source"]["power"], power);
            assert_eq!(view["objects"]["source"]["hp"], hp);
            let before = format!("{game:?}");
            game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "rejection preserves authority"
            );
        }
    }
}

#[test]
fn bp12_113_discards_before_roll_and_draws_three_on_four_through_six() {
    for number in printings("BP12-113") {
        for (seed, n) in [("dice-3", 3_usize), ("dice-0", 0)] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-112"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cost","card":"BP01-171"}]);
            let mut game = seeded(&initial, seed);
            let rolls = evolve(&mut game, false, false);
            assert_eq!(rolls.len(), 1);
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("cost"))
            );
            assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), n);
        }
    }
}

#[test]
fn ecp02_065_printed_permission_replaces_a_six_before_observers() {
    for number in printings("ECP02-065") {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":number},{"id":"roller","card":"BP21-083"}]);
        let mut game = seeded(&initial, "dice-3");
        let ability = game.internal_activated_sources("roller").unwrap()[0].definition;
        let play = game
            .activate_internal_card("roller", ability, PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(q) = &play else {
            panic!("printed reroll permission")
        };
        assert!(matches!(
            q.domain,
            InputDomain::DiceReroll {
                value: Nat(6),
                remaining: Nat(1)
            }
        ));
        assert_eq!(finish(&mut game, play, 0, true), vec![DieFace::Four]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8
        );
    }
}

#[test]
fn ecp02_066_draw_life_damage_and_opponents_maximum_attack_choice() {
    for number in printings("ECP02-066") {
        for (seed, face, hand, own, enemy) in [
            ("card-seed", 1_u8, 2_usize, 20_u8, 20_u8),
            ("dice-1", 2, 0, 23, 17),
            ("dice-0", 3, 0, 20, 20),
            ("dice-3", 6, 0, 20, 20),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":"ECP02-065"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"high-a","card":"BP01-171","state":{"power":8_u8}},
                {"id":"high-b","card":"BP01-171","state":{"power":8_u8}},
                {"id":"low","card":"BP01-171","state":{"power":1_u8}}
            ]);
            let mut game = seeded(&initial, seed);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("evolution event")
            };
            let evolution = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let CardPlay::Awaiting(reroll) = evolution else {
                panic!("evolved card grants its own reroll")
            };
            assert!(
                matches!(reroll.domain,InputDomain::DiceReroll{value:Nat(v),remaining:Nat(1)} if v==u64::from(face))
            );
            let mut result = game
                .answer_internal_card(reroll.actor, reroll.token, Answer::Integer(Nat(0)))
                .unwrap();
            if face == 3 {
                let CardPlay::Awaiting(choice) = result else {
                    panic!("opponent chooses a maximum attack follower")
                };
                assert_eq!(choice.actor, PlayerId { seat: 2 });
                let InputDomain::Targets(domain) = &choice.domain else {
                    panic!("opponent public choice")
                };
                assert_eq!(
                    domain.members.len(),
                    2,
                    "both tied maxima, never the weak follower"
                );
                result = game
                    .answer_internal_card(
                        choice.actor,
                        choice.token,
                        Answer::Targets(vec![domain.members[1].handle]),
                    )
                    .unwrap();
            }
            finish(&mut game, result, 0, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), hand);
            assert_eq!(view["P1"]["leader"]["life"], own);
            assert_eq!(view["P2"]["leader"]["life"], enemy);
            assert!(
                view["P2"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("low"))
            );
            assert_eq!(
                view["P2"]["cemetery"].as_array().unwrap().len(),
                usize::from(face == 3)
            );
        }
    }
}

#[test]
fn bp10_019_all_four_modes_include_the_paid_advance_entry() {
    for number in printings("BP10-019") {
        for mode in 0..4 {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"advance","card":"BP10-020"}]);
            let mut game = seeded(&initial, "card-seed");
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, mode, false);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"],
                if mode == 3 { 3_u8 } else { 7_u8 }
            );
            match mode {
                0 => {
                    let field = view["P1"]["field"].as_array().unwrap();
                    assert_eq!(field.len(), 2);
                    let token = field
                        .iter()
                        .find(|id| *id != "source")
                        .unwrap()
                        .as_str()
                        .unwrap();
                    assert_eq!(view["objects"][token]["card"], "BP17-T11");
                }
                1 => {
                    assert_eq!(view["objects"]["source"]["power"], 4_u8);
                    assert!(
                        view["objects"]["source"]["keywords"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|k| k == "突進")
                    );
                    assert!(
                        view["objects"]["source"]["keywords"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|k| k == "守護")
                    );
                }
                2 => assert_eq!(view["P2"]["leader"]["life"], 18_u8),
                3 => {
                    assert!(
                        view["P1"]["cemetery"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("source"))
                    );
                    assert!(
                        view["P1"]["field"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("advance"))
                    );
                    assert_eq!(view["P1"]["evolve_deck"], json!([]));
                }
                _ => panic!("only four printed modes"),
            }
        }
    }
}

#[test]
fn bp10_020_end_rolls_cover_all_branches_and_delay_loss_until_the_body_finishes() {
    for number in printings("BP10-020") {
        for (seed, expected, own, enemy, destroyed) in [
            (
                "dice-1",
                vec![DieFace::Two, DieFace::Three],
                20_u8,
                13_u8,
                true,
            ),
            ("dice-3", vec![DieFace::Six, DieFace::Four], 0, 0, false),
        ] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"acted":true}}]);
            if seed == "dice-3" {
                initial["players"]["P1"]["leader"]["life"] = json!(7_u8);
                initial["players"]["P2"]["leader"]["life"] = json!(7_u8);
            }
            let mut game = seeded(&initial, seed);
            for target in ["enemy", "P2.leader"] {
                assert_eq!(
                    game.internal_rule_action(
                        &json!({"do":"attack","attacker":"source","target":target})
                    )
                    .unwrap()
                    .outcome,
                    "cannot-attack"
                );
            }
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            assert_eq!(settle(&mut game), expected);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["leader"]["life"], own);
            assert_eq!(view["P2"]["leader"]["life"], enemy);
            let zone = if destroyed { "cemetery" } else { "field" };
            assert!(
                view["P2"][zone]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy"))
            );
            if seed == "dice-3" {
                assert_eq!(view["game"]["ended"], true);
                assert!(view["game"]["winner"].is_null());
            }
        }
    }
}

/// Q2730: with the dice history satisfied but no opposing follower, activation is illegal.
fn bp21_076_rejects_activation_without_an_opposing_follower(number: &str) {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":number},{"id":"roller","card":"BP21-083"},
        {"id":"material","card":"BP01-171"}
    ]);
    let mut game = seeded(&initial, "dice-3");
    activate(&mut game, "roller");
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    let before = format!("{game:?}");
    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "Q2730: no opposing follower means no payment or draw"
    );
}

#[test]
fn bp21_076_looks_discounts_the_next_academy_and_requires_own_turn_dice_history() {
    for number in printings("BP21-076") {
        for prior in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"roller","card":"BP21-083"},{"id":"material","card":"BP01-171"}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"academy","card":"BP21-087"},{"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
            let mut game = seeded(&initial, "dice-3");
            if prior {
                activate(&mut game, "roller");
            }
            let entry = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, entry, 0, false);
            let looked = game.projection(View::Referee).unwrap();
            assert_eq!(looked["P1"]["hand"], json!(["academy"]));
            assert_eq!(looked["P1"]["deck"], json!(["d3", "d2"]));
            let ability = game.internal_activated_sources("source").unwrap()[0].definition;
            if !prior {
                let before = format!("{game:?}");
                game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                    .unwrap_err();
                assert!(
                    format!("{game:?}") == before,
                    "rejection preserves authority"
                );
                activate(&mut game, "roller");
            }
            let academy = game
                .play_internal_card("academy", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, academy, 0, false);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                7_u8,
                "the next Academy original cost <=2 is discounted by two"
            );
            let activation = game
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap();
            let CardPlay::Awaiting(target) = activation else {
                panic!("formal follower target before payment")
            };
            let InputDomain::Targets(targets) = &target.domain else {
                panic!("enemy follower target")
            };
            let payment = game
                .answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![targets.members[0].handle]),
                )
                .unwrap();
            let CardPlay::Awaiting(cost) = payment else {
                panic!("other field card material")
            };
            let InputDomain::CostMaterials(materials) = &cost.domain else {
                panic!("field material cost")
            };
            assert!(
                !materials
                    .members
                    .iter()
                    .any(|m| m.card.as_deref() == Some(number.as_str())),
                "the source cannot pay its own other-card cost"
            );
            let material = materials
                .members
                .iter()
                .find(|m| m.card.as_deref() == Some("BP01-171"))
                .unwrap()
                .handle;
            let resolution = game
                .answer_internal_card(
                    cost.actor,
                    cost.token,
                    Answer::CostMaterials(vec![material]),
                )
                .unwrap();
            finish(&mut game, resolution, 0, false);
            let resolved = game.projection(View::Referee).unwrap();
            assert_eq!(resolved["objects"]["source"]["acted"], true);
            assert!(
                resolved["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material"))
            );
            assert_eq!(resolved["objects"]["enemy"]["hp"], 7_u8);
            assert_eq!(resolved["P1"]["hand"], json!(["d3"]));
        }
        bp21_076_rejects_activation_without_an_opposing_follower(&number);
    }
}

#[test]
fn cp01_081_main_begin_roll_has_damage_draw_and_self_destroy_branches() {
    for number in printings("CP01-081") {
        for (seed, face, life, hand, destroyed) in [
            ("card-seed", DieFace::One, 15_u8, 1_usize, false),
            ("dice-0", DieFace::Three, 20, 2, false),
            ("dice-3", DieFace::Six, 23, 1, true),
        ] {
            let mut initial = position();
            initial["turn"]["active"] = json!("P2");
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["deck"] =
                json!([{"id":"other-deck","card":"BP01-171"}]);
            let mut game = seeded(&initial, seed);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let mut rolls = settle(&mut game);
            while let Some(q) = game.internal_quick_choice().unwrap() {
                let play = game
                    .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                    .unwrap();
                rolls.extend(finish(&mut game, play, 0, false));
            }
            rolls.extend(settle(&mut game));
            assert_eq!(rolls, vec![face]);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["leader"]["life"], life);
            assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), hand);
            let zone = if destroyed { "cemetery" } else { "field" };
            assert!(
                view["P1"][zone]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
        }
    }
}
