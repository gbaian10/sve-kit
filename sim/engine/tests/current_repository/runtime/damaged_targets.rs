#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these real damage-trigger scenarios."
)]
use super::*;

fn position(lethal: bool, targets_remain: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"BP04-087", "state":{"entered_this_turn":false,"damage":if lethal { 4_u8 } else { 0 }}
    }]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-088"}]);
    let mut enemies = vec![json!({"id":"low","card":"BP01-038","state":{"acted":true}})];
    if targets_remain {
        enemies.push(json!({"id":"high","card":"BP04-117"}));
    }
    initial["players"]["P2"]["zones"]["field"] = json!(enemies);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "fenrir-damage",
    )
    .unwrap()
}

fn combat(game: &mut Game, attacker: &str, target: &str) {
    game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "defensive pass completes combat"
    );
}

fn trigger(game: &mut Game, printing: &str, line: u64) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(
        pending.actor,
        PlayerId { seat: 1 },
        "Fenrir's controller plays its ability"
    );
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("damage occurrence")
    };
    assert_eq!(options.len(), 1, "one damage occurrence is pending");
    assert_eq!(
        options[0].card, printing,
        "the effective printing supplies the trigger"
    );
    assert_eq!(
        options[0].line, line,
        "the ability keeps its printed source line"
    );
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn damage_target(game: &mut Game, printing: &str, line: u64) {
    let CardPlay::Awaiting(targets) = trigger(game, printing, line) else {
        panic!("formal target")
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "one opposing follower is required"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the ability targets exactly one follower"
    );
    assert_eq!(
        domain.members.len(),
        1,
        "only the surviving opposing follower is eligible"
    );
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        targets.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a foreign answer leaves pending authority unchanged"
    );
    let result = resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1,
        "the printed ability commits once"
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["high"]["damage"],
        3_u8,
        "the selected follower takes three damage"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional occurrence remains"
    );
}

#[test]
fn fenrir_damage_declares_a_target_after_surviving_or_lethal_own_turn_combat() {
    for lethal in [false, true] {
        let mut game = game(&position(lethal, true));
        combat(&mut game, "source", "low");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            if lethal { json!([]) } else { json!(["source"]) }
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if lethal { json!(["source"]) } else { json!([]) }
        );
        assert_eq!(state["P2"]["cemetery"], json!(["low"]));
        assert_eq!(state["objects"]["high"]["damage"], 0_u8);
        damage_target(&mut game, "BP04-087", 2);
    }
}

#[test]
fn fenrir_does_not_trigger_when_damaged_on_the_opponents_turn() {
    let mut initial = position(false, true);
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":false});
    let mut game = game(&initial);
    combat(&mut game, "low", "source");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional occurrence remains"
    );
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["damage"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["high"]["damage"],
        0_u8
    );
}

#[test]
fn fenrir_without_a_remaining_opposing_follower_cancels_its_occurrence() {
    let mut game = game(&position(false, false));
    combat(&mut game, "source", "low");
    assert!(matches!(
        trigger(&mut game, "BP04-087", 2),
        CardPlay::Cancelled
    ));
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional occurrence remains"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}

#[test]
fn evolved_fenrir_uses_the_new_printed_damage_ability() {
    let mut game = game(&position(false, true));
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no additional occurrence remains"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
        "form"
    );
    combat(&mut game, "source", "low");
    damage_target(&mut game, "BP04-088", 1);
}
