#![expect(
    clippy::panic,
    reason = "An unexpected question invalidates the real evolution scenario."
)]

use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "super-evolve-targets",
    )
    .unwrap()
}

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":7_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base}, {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037"}, {"id":"amulet","card":"BP13-087"}
    ]);
    initial
}

fn evolve(game: &mut Game, super_evolution: bool) {
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for its printed cost"
    );
}

fn select_head(game: &mut Game, super_head: bool) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected a real evolution trigger");
    };
    // Both facets share the super-evolution event; the printed line identifies the head.
    let option = options
        .iter()
        .find(|option| option.line == if super_head { 3 } else { 2 })
        .unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

fn resolve_head(game: &mut Game, super_head: bool) {
    let CardPlay::Awaiting(target) = select_head(game, super_head) else {
        panic!("expected a formal target before committing the printed ability");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("the printed head declares targets without a payment prompt");
    };
    assert_eq!(
        domain.members.len(),
        if super_head { 2 } else { 1 },
        "super evolution can target the amulet as well as the follower"
    );
    assert!(
        domain.members.iter().all(|member| member.player.seat == 2),
        "own field cards cannot be targeted"
    );
    let card = if super_head { "BP13-087" } else { "BP01-037" };
    let handle = domain.members.iter().find(|member| member.card.as_deref() == Some(card)).unwrap().handle;
    let before = game.projection(View::Referee).unwrap();
    for (actor, answer) in [
        (target.actor, vec![]),
        (PlayerId { seat: 2 }, vec![handle]),
    ] {
        assert!(
            game.answer_internal_card(actor, target.token, Answer::Targets(answer)).is_err(),
            "an omitted target or another actor cannot commit this ability"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "a rejected declaration preserves state"
        );
    }
    let CardPlay::Resolved(result) = game
        .answer_internal_card(target.actor, target.token, Answer::Targets(vec![handle]))
        .unwrap()
    else {
        panic!("the valid target commits the printed ability");
    };
    assert_eq!(
        result.progress.facts.iter().filter(|fact| matches!(fact, Fact::AbilityPlayed { .. })).count(),
        1,
        "only a valid declaration produces the ability-played fact"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn bp16_115_and_p26_declare_both_real_evolution_heads_in_either_order() {
    for (base, form) in [("BP16-114", "BP16-115"), ("BP16-P25", "BP16-P26")] {
        for (super_evolution, super_first) in [(false, false), (true, false), (true, true)] {
            let mut game = game(&position(base, form));
            evolve(&mut game, super_evolution);
            let evolved = game.projection(View::Referee).unwrap();
            assert_eq!(evolved["P1"]["pp"]["current"], 1_u8);
            assert_eq!(evolved["objects"]["enemy"]["damage"], 0_u8);
            assert_eq!(evolved["P2"]["cemetery"], json!([]));
            if super_evolution {
                resolve_head(&mut game, super_first);
                resolve_head(&mut game, !super_first);
            } else {
                resolve_head(&mut game, false);
            }
            assert!(game.internal_pending_choice().unwrap().is_none(), "all printed heads resolved");
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(after["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(after["P1"]["pp"]["current"], 1_u8);
            assert_eq!(after["P1"]["sep"], u8::from(!super_evolution));
            assert_eq!(after["P1"]["evolution"], json!(["form"]));
            assert_eq!(after["objects"]["source"]["evolved_with"], "form");
            assert_eq!(after["objects"]["source"]["power"], 4_u8 + u8::from(super_evolution));
            assert_eq!(after["P2"]["cemetery"], if super_evolution { json!(["amulet"]) } else { json!([]) });
        }
    }
}

#[test]
fn bp16_115_super_evolution_without_opposing_targets_cancels_only_the_abilities() {
    let mut initial = position("BP16-114", "BP16-115");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    evolve(&mut game, true);
    for super_head in [true, false] {
        assert_eq!(
            select_head(&mut game, super_head),
            CardPlay::Cancelled,
            "a required opposing target must exist before the ability is played"
        );
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none(), "both impossible heads are cleared");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 1_u8);
    assert_eq!(after["P1"]["sep"], 0_u8);
    assert_eq!(after["P1"]["evolution"], json!(["form"]));
    assert_eq!(after["P1"]["field"], json!(["source", "ally"]));
}
