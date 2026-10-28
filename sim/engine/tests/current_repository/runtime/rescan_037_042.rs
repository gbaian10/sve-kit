#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real card scenarios."
)]
use super::*;
use sve_engine::game::internal::InstructionOutcome;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, field: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][if field { "field" } else { "hand" }] =
        json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}
    ]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "rescan-037-042",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = question.domain else {
                panic!("unexpected real card decision");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    let CardPlay::Entered(entry) = finish(game, play) else {
        panic!("the real follower must enter the field");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game, card: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("printed trigger");
    };
    assert_eq!(options.len(), 1, "one real card trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the trigger belongs to the selected printing"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let result = resolved(finish(game, play));
    game.resume_internal_card().unwrap();
    result
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = position(card, true);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let less = cost.checked_sub(1).unwrap();
    let mut insufficient = initial.clone();
    insufficient["players"]["P1"]["pp"]["current"] = json!(less);
    let mut denied = game(&insufficient);
    if let Ok(step) = denied.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":less}}),
    ) {
        assert_ne!(
            step.outcome, "resolved",
            "less than the printed cost cannot evolve"
        );
    }
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        less,
        "rejected evolution preserves PP"
    );
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the exact printed cost permits evolution"
    );
    assert!(
        step.events.iter().any(|event| event["kind"] == "進化"
            && event["object"] == "source"
            && event["with"] == "form"),
        "the real base uses the matching evolution resource"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "evolution pays its complete printed cost"
    );
}

fn ward(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    let attack = json!({"do":"attack","attacker":"attacker","target":"P2.leader"});
    assert_eq!(
        game(&initial)
            .internal_rule_action(&attack)
            .unwrap()
            .outcome,
        "cannot-attack",
        "rested Ward protects its leader"
    );
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    assert_eq!(
        game(&initial)
            .internal_rule_action(&attack)
            .unwrap()
            .outcome,
        "resolved",
        "the same attack is legal without rested Ward"
    );
}

fn attack_leader(game: &mut Game, life: u8) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "Storm permits the immediate leader attack"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "Quick Pass resumes the attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        life,
        "the real follower power damages the leader"
    );
}

#[test]
fn csd02a_009_and_alias_evolve_ward_and_fanfare_damage() {
    for card in ["CSD02a-009", "CSD02a-P06"] {
        evolve(card, "CSD02a-010", 1);
        ward(card);
        let mut initial = position(card, false);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        enter(&mut game);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    }
}

#[test]
fn csd02b_007_and_alias_evolve_for_two_pp() {
    for card in ["CSD02b-007", "CSD02b-P04"] {
        evolve(card, "CSD02b-008", 2);
    }
}

#[test]
fn csd02b_010_and_alias_evolve_and_attack_on_the_entry_turn() {
    for card in ["CSD02b-010", "CSD02b-P07"] {
        evolve(card, "CSD02b-011", 2);
        let mut game = game(&position(card, false));
        enter(&mut game);
        attack_leader(&mut game, 16);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            5_u8
        );
    }
}

#[test]
fn csd02b_011_and_alias_damage_on_real_evolution_and_keep_storm() {
    for card in ["CSD02b-011", "CSD02b-P08"] {
        let mut initial = position("CSD02b-010", true);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        attack_leader(&mut game, 15);
    }
}

#[test]
fn csd02c_005_and_alias_privately_look_at_one_top_card_without_moving_it() {
    for card in ["CSD02c-005", "CSD02c-P02"] {
        evolve(card, "CSD02c-006", 1);
        let mut game = game(&position(card, false));
        assert!(game.projection(View::P1).unwrap()["objects"]["first"].is_null());
        enter(&mut game);
        let result = trigger(&mut game, card);
        let looked = result
            .progress
            .receipts
            .values()
            .find_map(|receipt| {
                if let InstructionOutcome::Look(outcome) = receipt {
                    Some(outcome)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(looked.looked.len(), 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["first", "second"]));
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        let own = game.projection(View::P1).unwrap();
        assert_eq!(own["objects"]["first"]["card"], "BP01-037");
        assert!(own["objects"]["second"].is_null());
        assert!(game.projection(View::P2).unwrap()["objects"]["first"].is_null());
    }
}

#[test]
fn csd02c_010_and_alias_evolve_for_one_pp() {
    for card in ["CSD02c-010", "CSD02c-P07"] {
        evolve(card, "CSD02c-011", 1);
    }
}

#[test]
fn csd03b_009_and_alias_draw_one_and_protect_with_ward() {
    for card in ["CSD03b-009", "CSD03b-P06"] {
        ward(card);
        let mut game = game(&position(card, false));
        enter(&mut game);
        trigger(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn dsd01a_010_and_alias_evolve_for_one_pp() {
    for card in ["DSD01a-010", "DSD01a-P04"] {
        evolve(card, "DSD01a-011", 1);
    }
}

#[test]
fn dsd01b_009_and_available_aliases_choose_capped_maximum_pp_or_draw() {
    // PR-599 is reviewed but absent from the pinned engine input.
    for card in ["DSD01b-009", "PR-010", "PR-209", "SD04-002", "SD08-010"] {
        for (maximum, mode, expected_maximum, expected_hand, expected_deck) in [
            (8_u8, 1_u64, 9_u8, json!([]), json!(["first", "second"])),
            (8, 2, 8, json!(["first"]), json!(["second"])),
            (10, 1, 10, json!([]), json!(["first", "second"])),
        ] {
            let mut initial = position(card, false);
            initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":maximum});
            let mut game = game(&initial);
            let CardPlay::Awaiting(question) = game.play_internal_card("source", actor()).unwrap()
            else {
                panic!("mode choice");
            };
            assert!(matches!(question.domain, InputDomain::Modes { .. }));
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Modes(vec![ModeId(mode)]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"],
                json!({"current":3_u8,"max":expected_maximum})
            );
            assert_eq!(state["P1"]["hand"], expected_hand);
            assert_eq!(state["P1"]["deck"], expected_deck);
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            assert_eq!(state["P2"]["pp"], json!({"current":10_u8,"max":10_u8}));
        }
    }
}
