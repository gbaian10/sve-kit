use super::*;

fn position(number: &str, target: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    if target {
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"target","card":"BP01-037","state":{"acted":true,"power":0_u8,"hp":1_u8,"max_hp":1_u8}}
        ]);
    }
    initial
}

fn strike(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the real attack reaches its Quick window"
    );
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

#[expect(
    clippy::panic,
    reason = "A damage occurrence must use the public pending domain."
)]
fn recover(game: &mut Game, number: &str, amount: u8) {
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["leader"]["life"], 10_u8,
        "Drain waits for its own resolution"
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Drain pending")
    };
    assert_eq!(
        options.len(),
        1_usize,
        "one Drain occurrence is queued per attack hit"
    );
    assert_eq!(
        options[0].card, number,
        "the occurrence keeps its printed provider"
    );
    assert_eq!(
        options[0].line, 1_u64,
        "the occurrence keeps the printed keyword line"
    );
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8.checked_add(amount).unwrap(),
        "Drain recovers the actual attack damage exactly once"
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional Drain occurrence remains"
    );
}

#[test]
fn bp13_t05_attack_recovers_actual_damage_even_above_defenders_remaining_life() {
    let mut game = game(&position("BP13-T05", true));
    strike(&mut game, "source", "target");
    recover(&mut game, "BP13-T05", 3);
}

#[test]
fn bp16_088_storm_attack_to_leader_uses_the_same_drain_occurrence() {
    let mut initial = position("BP16-088", false);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut game = game(&initial);
    strike(&mut game, "source", "P2.leader");
    recover(&mut game, "BP16-088", 5);
}

#[test]
fn cp02_030_all_printings_recover_after_bane_combat() {
    for number in ["CP02-030", "CP02-P24"] {
        let mut game = game(&position(number, true));
        strike(&mut game, "source", "target");
        recover(&mut game, number, 3);
    }
}

#[test]
fn drain_does_not_recover_counterattack_damage_zero_damage_or_silenced_keywords() {
    for (defending, zero, silenced) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut initial = position("BP16-088", true);
        initial["players"]["P1"]["zones"]["field"][0]["state"] =
            json!({"acted":defending,"power":if zero {0_u8} else {4_u8},"silenced":silenced});
        if defending {
            initial["turn"]["active"] = json!("P2");
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"power":0_u8});
        }
        let mut game = game(&initial);
        if defending {
            strike(&mut game, "target", "source");
        } else {
            strike(&mut game, "source", "target");
        }
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "no additional Drain occurrence remains"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8,
            "counterattack, zero damage and lost keywords never recover life"
        );
    }
}

#[test]
fn drain_pending_survives_the_attackers_lethal_counterattack() {
    for (number, amount, token) in [("BP13-T05", 3, true), ("BP16-088", 5, false)] {
        let mut initial = position(number, true);
        initial["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(10_u8);
        let mut game = game(&initial);
        strike(&mut game, "source", "target");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!([]), "the attacker was destroyed");
        assert_eq!(
            view["P1"]["cemetery"],
            if token { json!([]) } else { json!(["source"]) },
            "tokens disappear after leaving the field"
        );
        recover(&mut game, number, amount);
    }
}
