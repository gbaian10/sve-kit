#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the complete printed-card scenarios."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "attack-batch",
    )
    .unwrap()
}

fn attack(game: &mut Game, source: &str, target: &str) -> String {
    game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":target}))
        .unwrap()
        .outcome
}

fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("Quick did not finish");
}

fn end_turn(game: &mut Game) {
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the main phase ends normally"
    );
    for _ in 0_u8..4 {
        if game.projection(View::Referee).unwrap()["turn"]["active"] != active {
            return;
        }
        if game.internal_quick_choice().unwrap().is_some() {
            pass_quick(game);
        } else {
            assert_eq!(
                game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
                    .unwrap()
                    .outcome,
                "resolved",
                "the real Ward follower may remain standing at its end-phase choice"
            );
        }
    }
    panic!("The end phase did not finish");
}

fn decks(initial: &mut Value) {
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..4)
                .map(|i| json!({"id":format!("{seat}-next-{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
}

fn entered(game: &mut Game, play: CardPlay) {
    let CardPlay::Entered(entry) = play else {
        panic!("the complete real follower enters")
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn fanfare(game: &mut Game, number: &str, target_card: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed fanfare")
    };
    assert_eq!(
        options[0].card, number,
        "the real printing supplies the fanfare"
    );
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("declared target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("public target")
    };
    let target = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(target_card))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![target]))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn shoko_all_printings_cannot_attack_and_heal_at_own_end_phase() {
    for number in ["CP02-083", "CP02-P68", "CSD02c-014"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        decks(&mut initial);
        let mut game = game(&initial);
        for target in ["P2.leader", "enemy"] {
            assert_eq!(
                attack(&mut game, "source", target),
                "cannot-attack",
                "Shoko cannot attack either opposing recipient"
            );
        }
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("own end heal")
        };
        assert_eq!(
            options[0].line, 3,
            "the printed end-phase ability is pending"
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 11_u8,
            "the controller heals once at its own end phase"
        );
        assert_eq!(
            view["P2"]["leader"]["life"], 20_u8,
            "the other leader is unaffected"
        );
    }
}

#[test]
fn erin_locks_both_attack_targets_only_through_the_targets_next_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP16-009"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    decks(&mut initial);
    let mut game = game(&initial);
    let CardPlay::Awaiting(q) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ward entry")
    };
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
        .unwrap();
    entered(&mut game, play);
    fanfare(&mut game, "BP16-009", "BP01-037");
    end_turn(&mut game);
    for target in ["P1.leader", "ally"] {
        assert_eq!(
            attack(&mut game, "enemy", target),
            "cannot-attack",
            "the selected enemy cannot attack any opponent during its next turn"
        );
    }
    end_turn(&mut game);
    end_turn(&mut game);
    assert_eq!(
        attack(&mut game, "enemy", "P1.leader"),
        "resolved",
        "the prohibition expires at the end of the captured next turn"
    );
}

#[test]
fn alarm_all_printings_stand_a_vanguard_and_only_forbid_leader_attacks_this_turn() {
    for number in ["CP03-120", "CP03-P83"] {
        let ally_number = if number == "CP03-120" {
            "CP03-P83"
        } else {
            "CP03-120"
        };
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":ally_number,"state":{"acted":true}}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"power":0_u8}}]);
        decks(&mut initial);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        entered(&mut game, play);
        fanfare(&mut game, number, ally_number);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["ally"]["acted"],
            false,
            "the chosen Vanguard stands"
        );
        assert_eq!(
            attack(&mut game, "ally", "P2.leader"),
            "cannot-attack",
            "the installed permission forbids the leader"
        );
        assert_eq!(
            attack(&mut game, "ally", "enemy"),
            "resolved",
            "follower attacks remain legal"
        );
        pass_quick(&mut game);
        end_turn(&mut game);
        end_turn(&mut game);
        assert_eq!(
            attack(&mut game, "ally", "P2.leader"),
            "resolved",
            "the leader prohibition expires this turn"
        );
    }
}
