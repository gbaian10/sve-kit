#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the history and target scenarios."
)]
use super::event_cemetery::{finish_checks, move_named};
use super::event_subject_capture::{enter, finish, position, prepared, prepared_program, resolve};
use super::*;

fn departure(destroy: bool, to: OwnerZone) -> Action {
    let move_ = move_named(ZoneKind::Field, to, "subject soldier");
    if destroy {
        let Action::Move { objects, .. } = move_ else {
            panic!("departure fixture");
        };
        Action::Destroy { objects }
    } else {
        move_
    }
}

#[test]
fn flame_soldier_printings_count_only_committed_own_destruction_before_fanfare() {
    for number in ["BP02-033", "BP02-P10"] {
        for (side, destroy, to, expected) in [
            ("P1", true, OwnerZone::Cemetery, 4_u8),
            ("P1", false, OwnerZone::Cemetery, 1),
            ("P1", false, OwnerZone::Banish, 1),
            ("P2", true, OwnerZone::Cemetery, 1),
        ] {
            let mut initial = position(number, true);
            initial["players"][side]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":"subject-soldier"}));
            let mut game = prepared(&initial, vec![departure(destroy, to)]);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            resolve(&mut game, number, 1, None);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                6_u8 - expected
            );
        }
    }
}

#[test]
fn setus_printings_heal_at_end_and_use_departure_history_after_the_subject_returns() {
    for number in ["BP07-005", "PR-181"] {
        for (to, expected) in [(OwnerZone::Cemetery, true), (OwnerZone::Banish, false)] {
            let mut initial = position(number, false);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":"subject-soldier"}));
            let mut actions = vec![departure(false, to)];
            if expected {
                actions.push(enter(ZoneKind::Cemetery, "subject soldier"));
            }
            let mut game = prepared(&initial, actions);
            activate(&mut game, "P1");
            finish_checks(&mut game);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            resolve(&mut game, number, 1, None);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["leader"]["life"], 14_u8);
            assert_eq!(
                state["objects"]["source"]["power"],
                3_u8 + 2 * u8::from(expected)
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                5_u8 + 2 * u8::from(expected)
            );
        }
    }
}

#[test]
fn liberte_evolution_reads_earlier_field_cemetery_history_and_always_draws() {
    for (to, expected) in [(OwnerZone::Cemetery, 4_u8), (OwnerZone::Hand, 3)] {
        let mut initial = position("history-base", false);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-079"}]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"subject-soldier"}));
        let mut game = prepared(&initial, vec![departure(false, to)]);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolve(&mut game, "BP13-079", 1, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 6_u8 - expected);
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("draw"))
        );
    }
}

fn begin_attack(game: &mut Game, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack declaration commits before either pending effect resolves"
    );
}

#[test]
fn moon_al_miraj_printings_queue_on_leaders_but_only_buff_against_a_current_follower() {
    for number in ["BP01-126", "BP01-SL16"] {
        for (target, expected) in [("enemy", 6_u8), ("P2.leader", 4)] {
            let initial = position(number, false);
            let mut game = prepared(&initial, vec![]);
            begin_attack(&mut game, target);
            resolve(&mut game, number, 1, None);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
                expected
            );
        }
    }
}

#[test]
fn evolved_suttung_buffs_both_stats_only_when_the_frozen_recipient_is_a_follower() {
    for (target, expected) in [("enemy", 7_u8), ("P2.leader", 5)] {
        let mut initial = position("combat-base", false);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-111"}]);
        let mut game = prepared(&initial, vec![]);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        begin_attack(&mut game, target);
        resolve(&mut game, "BP09-111", 1, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], expected);
        assert_eq!(state["objects"]["source"]["hp"], expected);
    }
}

#[test]
fn momo_printings_read_the_attack_target_life_at_resolution_before_draw_and_discard() {
    for number in ["BP03-027", "BP03-P07"] {
        for (target, hp, expected) in [
            ("enemy", 5_u8, true),
            ("enemy", 6, false),
            ("P2.leader", 5, false),
        ] {
            let mut initial = position(number, false);
            initial["players"]["P2"]["zones"]["field"][0]["state"]["hp"] = json!(hp);
            let mut game = prepared(&initial, vec![]);
            begin_attack(&mut game, target);
            resolve(&mut game, number, 1, None);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("draw")),
                expected
            );
            assert_eq!(
                projected_zone_count(&state["P1"]["deck"]),
                u64::from(!expected)
            );
        }
    }
}

#[test]
fn liliums_dragon_printings_set_the_actual_attack_target_without_changing_the_attacker() {
    for number in ["BP21-P67", "BP21-T07"] {
        let initial = position(number, false);
        let mut game = prepared(&initial, vec![]);
        begin_attack(&mut game, "enemy");
        resolve(&mut game, number, 1, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 1_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
        assert_eq!(state["objects"]["source"]["power"], 5_u8);
    }
}

fn observer_tree(body: &Value) -> Value {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["auto"] = json!({"on":{"attacked":{"who":"self","sel":{"kind":"follower"}}},"do":body});
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"producer":card});
    tree
}

#[test]
fn an_attack_target_is_not_followed_after_leaving_and_reentering_during_an_earlier_trigger() {
    let mut initial = position("BP01-126", false);
    // Ownership stays fixed while the initial explicit history places it under opposing control.
    initial["players"]["P2"]["zones"]["field"][0]["owner"] = json!("P1");
    let mut program = lowerer()
        .lower(&observer_tree(
            &json!([{"buff":{"on":"this","power":1_u8}}]),
        ))
        .unwrap()["producer"]
        .card()
        .clone();
    let body = event_subject_capture::production(vec![
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "capture low"),
        enter(ZoneKind::Cemetery, "capture low"),
    ])
    .card()
    .abilities[0]
        .node
        .body
        .clone();
    program.abilities[0].node.body = body;
    let source = program.abilities[0].node.source.clone();
    let span = program.abilities[0].source.clone();
    for step in &mut program.abilities[0].node.body.as_mut().unwrap().steps {
        let Node::Instruction { instruction } = &mut step.node else {
            panic!("external movement instruction");
        };
        instruction.source = source.clone();
        step.source = span.clone();
    }
    let mut game = prepared_program(&initial, &WellFormedCardProgram::check(program).unwrap());
    begin_attack(&mut game, "enemy");
    resolve(&mut game, "producer", 2, None);
    resolve(&mut game, "BP01-126", 1, None);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy"))
    );
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
}

#[test]
fn momo_reads_changed_life_when_its_attack_trigger_resolves() {
    let initial = position("BP03-027", false);
    let tree = observer_tree(&json!([{"set_stat":{"on":"attack_target","hp":5_u8}}]));
    let mut game = prepared_program(&initial, &lowerer().lower(&tree).unwrap()["producer"]);
    begin_attack(&mut game, "enemy");
    resolve(&mut game, "producer", 2, None);
    resolve(&mut game, "BP03-027", 1, None);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("draw"))
    );
}

fn history_probe(query: &Value) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability["activate"] = json!({"cost":{"pp":1_u8},"do":[{"if":{"cond":{"history":query},"then":[{"draw":{"who":"self","n":1_u8}}]}}]});
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"producer":card});
    lowerer().lower(&tree).unwrap()["producer"].clone()
}

#[test]
fn attack_and_evolution_history_are_recorded_without_a_matching_field_observer() {
    for (event, own) in [
        ("attacked", true),
        ("attacked", false),
        ("evolved", true),
        ("evolved", false),
    ] {
        let mut initial = position("combat-base", false);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-111"}]);
        let probe = history_probe(
            &json!({"event":event,"window":"this_turn","who":if own {"self"} else {"opp"},"sel":{"kind":"follower"},"distinct":"subjects"}),
        );
        let mut game = prepared_program(&initial, &probe);
        if event == "evolved" {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
        } else {
            begin_attack(&mut game, "P2.leader");
            assert!(game.internal_pending_choice().unwrap().is_none());
            let question = game.internal_quick_choice().unwrap().unwrap();
            let play = game
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
            finish(&mut game, play);
        }
        activate(&mut game, "P1");
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            if own { json!(["draw"]) } else { json!([]) }
        );
    }
}

#[test]
fn history_rejects_unimplemented_cutoffs_instead_of_reading_the_current_point() {
    for extra in [json!({"at":"event"}), json!({"exclude":"this_execution"})] {
        let mut query = json!({"event":"attacked","window":"this_turn"});
        query
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut tree = expanded::tree().clone();
        let mut card = tree["cards"]["BP01-041"].clone();
        card["abilities"][0]["activate"]["do"] =
            json!([{"if":{"cond":{"history":query},"then":[{"draw":{"who":"self","n":1_u8}}]}}]);
        tree["cards"] = json!({"history-boundary":card});
        assert_eq!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct"
        );
    }
}
