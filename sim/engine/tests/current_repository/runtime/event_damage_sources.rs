use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-damage-sources",
    )
    .unwrap()
}

fn resolve_printed(game: &mut Game, number: &str, line: u64) {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    assert!(
        matches!(&question.domain, InputDomain::Pending(_)),
        "a real printed ability is pending"
    );
    if let InputDomain::Pending(options) = question.domain {
        assert_eq!(
            options.len(),
            1,
            "only the expected printed occurrence is pending"
        );
        assert_eq!(
            (&*options[0].card, options[0].line),
            (number, line),
            "the occurrence keeps its printing and line"
        );
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(finish(game, play));
    }
    finish_checks(game);
}

fn no_pending(game: &mut Game) {
    finish_checks(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional occurrence survives"
    );
}

fn enemies() -> Value {
    json!([
        {"id":"enemy-a","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"power":0_u8}},
        {"id":"enemy-b","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"power":0_u8}}
    ])
}

#[test]
fn all_takumi_printings_revive_only_a_cheap_passion_follower_then_rush_a_standing_enemy() {
    for printing in ["ECP02-057", "ECP02-P30"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"takumi","card":printing}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"revived","card":"CP02-047"},
            {"id":"expensive","card":printing},
            {"id":"wrong-trait","card":"CP02-014"},
            {"id":"spell","card":"CP02-041"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = enemies();
        let mut game = actual(&initial);
        assert!(matches!(
            game.play_internal_card("takumi", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut game);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let options = if let InputDomain::Pending(options) = &question.domain {
            Some(options)
        } else {
            None
        }
        .unwrap();
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let targets = if let CardPlay::Awaiting(targets) = play {
            Some(targets)
        } else {
            None
        }
        .unwrap();
        let domain = if let InputDomain::Targets(domain) = &targets.domain {
            Some(domain)
        } else {
            None
        }
        .unwrap();
        assert_eq!(domain.members.len(), 1);
        let response = game
            .answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        resolved(finish(&mut game, response));
        no_pending(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert!(
            before["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "revived")
        );
        assert_eq!(before["P1"]["pp"]["current"], 4_u8);
        assert_eq!(before["objects"]["enemy-a"]["acted"], false);
        game.internal_rule_action(&json!({"do":"attack","attacker":"takumi","target":"enemy-a"}))
            .unwrap();
        let quick = game.internal_quick_choice().unwrap().unwrap();
        assert!(matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ));
        resolve_printed(&mut game, printing, 2);
        no_pending(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy-a"]["damage"], 5_u8);
        assert_eq!(after["P2"]["leader"]["life"], 19_u8);
    }
}

#[test]
fn all_takumi_printings_observe_a_real_demas_fanfare_and_one_two_recipient_lesson_batch() {
    for printing in ["ECP02-057", "ECP02-P30"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"takumi","card":printing}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"ranko","card":"CP02-069"}]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"lesson-a","card":"PR-155"},{"id":"lesson-b","card":"PR-156"}]);
        initial["players"]["P2"]["zones"]["field"] = enemies();
        let mut game = actual(&initial);
        assert!(matches!(
            game.play_internal_card("ranko", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolve_printed(&mut game, "CP02-069", 2);
        resolve_printed(&mut game, printing, 2);
        no_pending(&mut game);
        let definition = selection_with_rules().current["CP02-069"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| {
                ability.node.kind == AbilityKind::Activated
                    && matches!(
                        &ability.node.source.origin,
                        AbilityOrigin::Printed { line, .. } if line.0 == 3
                    )
            })
            .unwrap()
            .node
            .id;
        let play = game
            .activate_internal_card("ranko", definition, PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play));
        resolve_printed(&mut game, printing, 2);
        no_pending(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["leader"]["life"], 18_u8);
        assert_eq!(after["objects"]["enemy-a"]["damage"], 4_u8);
        assert_eq!(after["objects"]["enemy-b"]["damage"], 2_u8);
        assert_eq!(after["P1"]["ex"], json!([]));
        assert_eq!(after["P1"]["pp"]["current"], 5_u8);
    }
}

#[test]
fn all_takumi_printings_ignore_a_real_demas_spell_in_resolution() {
    for printing in ["ECP02-057", "ECP02-P30"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"takumi","card":printing}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-041"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"CP02-047"}]);
        initial["players"]["P2"]["zones"]["field"] = enemies();
        let mut game = actual(&initial);
        let play = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play));
        no_pending(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy-a"]["damage"], 5_u8);
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
        assert_eq!(after["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn all_blockade_printings_observe_kinnara_once_per_own_turn_even_when_it_kills_the_recipient() {
    for printing in ["CP03-064", "CP03-SL13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"blockade","card":printing}]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"kinnara-a","card":"CP03-075"},{"id":"kinnara-b","card":"CP03-075"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-037"},{"id":"left","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"victim-a","card":"BP01-037"},{"id":"victim-b","card":"BP01-037"}]);
        let mut game = actual(&initial);
        let first_damage = game
            .play_internal_card("kinnara-a", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, first_damage));
        resolve_printed(&mut game, printing, 2);
        no_pending(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!(["left"])
        );
        let second_damage = game
            .play_internal_card("kinnara-b", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, second_damage));
        no_pending(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
        assert_eq!(after["P2"]["cemetery"].as_array().unwrap().len(), 2);
        assert_eq!(after["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn all_blockade_printings_keep_the_opposing_quick_spell_trigger_after_the_spell_kills_them() {
    for printing in ["CP03-064", "CP03-SL13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"blockade","card":printing,"state":{"entered_this_turn":false}}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"snipe","card":"BP01-179"}]);
        let mut game = actual(&initial);
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"blockade","target":"P2.leader"}),
        )
        .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let options = if let InputDomain::Quick { options, .. } = &question.domain {
            Some(options)
        } else {
            None
        }
        .unwrap();
        let option = options.iter().find(|q| q.card == "BP01-179").unwrap();
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(option.handle)),
            )
            .unwrap();
        resolved(finish(&mut game, play));
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["blockade"])
        );
        resolve_printed(&mut game, printing, 1);
        no_pending(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}

#[test]
fn all_blockade_printings_ignore_spells_played_on_the_opponents_own_turn() {
    for printing in ["CP03-064", "CP03-SL13"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"blockade","card":printing}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"snipe","card":"BP01-179"}]);
        let mut game = actual(&initial);
        let play = game
            .play_internal_card("snipe", PlayerId { seat: 2 })
            .unwrap();
        resolved(finish(&mut game, play));
        no_pending(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["cemetery"], json!(["blockade"]));
        assert_eq!(after["P2"]["leader"]["life"], 20_u8);
    }
}
