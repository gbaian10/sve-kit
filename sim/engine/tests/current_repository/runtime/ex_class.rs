use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"storm","card":number},{"id":"mystic","card":"BP01-056"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"dragon-a","card":"BP01-092"},{"id":"dragon-b","card":"BP01-092"},
        {"id":"other-a","card":"BP01-171"},{"id":"other-b","card":"BP01-171"},
        {"id":"other-c","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    initial
}
fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-class",
    )
    .unwrap()
}

#[test]
fn gale_fanfare_moves_twice_and_dragon_cards_do_not_consume_the_non_dragon_quota() {
    for (number, member) in [
        ("BP21-071", "BP01-171"),
        ("BP21-P38", "BP01-171"),
        ("BP21-071", "BP01-056"),
        ("BP21-071", "BP02-091"),
    ] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["deck"][2]["card"] = json!(member);
        let mut game = build(&initial);
        let entered = game
            .play_internal_card("storm", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entered);
        finish_checks(&mut game);
        let fanfare = pending(&mut game, number);
        resolved(finish(&mut game, fanfare));
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["dragon-a", "dragon-b"])
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
        let spell = game
            .play_internal_card("mystic", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, spell));
        finish_checks(&mut game);
        let trigger = pending(&mut game, number);
        resolved(finish(&mut game, trigger));
        finish_checks(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn gale_observes_the_opponents_turn_and_its_quota_resets_on_the_next_actual_turn() {
    let mut initial = position("BP21-071");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mystic","card":"BP01-056"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"storm","card":"BP21-071"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"again","card":"BP01-056"},
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},
        {"id":"c","card":"BP01-171"},{"id":"d","card":"BP01-171"},
        {"id":"e","card":"BP01-171"},{"id":"f","card":"BP01-171"},
        {"id":"g","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"BP01-171"}]);
    let mut game = build(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick entry")
    };
    let spell = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap();
    resolved(finish(&mut game, spell));
    finish_checks(&mut game);
    let triggered = pending(&mut game, "BP21-071");
    resolved(finish(&mut game, triggered));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    damage_prevent::pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    let next_turn_spell = game
        .play_internal_card("again", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, next_turn_spell));
    finish_checks(&mut game);
    let next_turn_trigger = pending(&mut game, "BP21-071");
    resolved(finish(&mut game, next_turn_trigger));
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 14_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
}

#[test]
fn gale_cannot_heal_without_a_legal_opposing_follower_target() {
    let mut initial = position("BP21-071");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mystic","card":"BP01-056"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"storm","card":"BP21-071"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = build(&initial);
    let spell = game
        .play_internal_card("mystic", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, spell));
    finish_checks(&mut game);
    assert!(matches!(
        pending(&mut game, "BP21-071"),
        CardPlay::Cancelled
    ));
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}

#[test]
fn gale_fixed_repeat_obeys_live_ex_capacity_without_moving_the_second_top() {
    let mut initial = position("BP21-071");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"old-a","card":"BP01-171"},{"id":"old-b","card":"BP01-171"},
        {"id":"old-c","card":"BP01-171"},{"id":"old-d","card":"BP01-171"}
    ]);
    let mut game = build(&initial);
    let entered = game
        .play_internal_card("storm", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, entered);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP21-071");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["deck"][0], "dragon-b");
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn an_ex_class_exclusion_requires_its_exact_event_and_the_information_presence_guard() {
    let program = &selection_with_rules().current["BP21-071"].program;
    for wrong_event in [false, true] {
        let mut raw = program.card().clone();
        let observer = raw
            .abilities
            .iter_mut()
            .find(|ability| ability.source.line == 1)
            .unwrap();
        let scope = observer.node.body.as_ref().unwrap().scope;
        let TriggerCondition::Event {
            pattern: EventPattern::EnterEx { binding, predicate },
        } = &mut observer.node.trigger.as_mut().unwrap().on
        else {
            panic!("EX subscription")
        };
        if wrong_event {
            *binding = Binding::new(999, scope);
        } else {
            let Expr::Compute(operation) = predicate else {
                panic!("canonical filter")
            };
            let Predicate::And { predicates } = operation.as_mut() else {
                panic!("canonical conjunction")
            };
            predicates.remove(1);
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
}
