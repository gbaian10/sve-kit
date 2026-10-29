#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real attack scenario."
)]

use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "evolved-attack-aoe",
    )
    .unwrap()
}

fn position(base: &str, form: &str, followers: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base}, {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut opponents = vec![json!({"id":"amulet","card":"BP13-087"})];
    if followers {
        opponents.extend([
            json!({"id":"high","card":"BP04-117"}),
            json!({"id":"low","card":"BP01-037"}),
        ]);
    }
    initial["players"]["P2"]["zones"]["field"] = json!(opponents);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"ex-token","card":"BP01-T03"}]);
    initial
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for one PP"
    );
    assert!(game.internal_pending_choice().unwrap().is_none(), "this form has no evolution trigger");
}

fn resolve_attack(game: &mut Game, printed_line: u64) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"source","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real follower declares its attack"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed attack ability");
    };
    assert_eq!(options.len(), 1, "only this attack ability is pending");
    assert_eq!(options[0].line, printed_line, "the effective print supplies the ability");
    let CardPlay::Resolved(activation) = game
        .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("all opposing followers require no target choice");
    };
    assert_eq!(
        activation.progress.facts.iter().filter(|fact| matches!(fact, Fact::AbilityPlayed { .. })).count(),
        1,
        "the printed attack ability is played once"
    );
    game.resume_internal_card().unwrap();
}

fn finish_combat(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(question.actor, PlayerId { seat: 2 }, "the defending player receives the Quick window");
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None)).unwrap(),
            CardPlay::Continued(_)
        ),
        "the defensive Pass continues combat through the rule procedure"
    );
}

#[test]
fn bp04_118_all_printings_replace_the_base_attack_aoe_after_real_evolution() {
    for (base, form) in [
        ("BP04-117", "BP04-118"),
        ("BP04-SL22", "BP04-SL23"),
        ("BP04-117", "BP04-U07"),
    ] {
        for evolved in [false, true] {
            let mut game = game(&position(base, form, true));
            if evolved {
                evolve(&mut game);
            }
            resolve_attack(&mut game, if evolved { 1 } else { 2 });
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["high"]["damage"], if evolved { 5_u8 } else { 3_u8 });
            assert_eq!(after["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(after["objects"]["amulet"]["damage"], 0_u8);
            assert_eq!(after["objects"]["ex-token"]["damage"], 0_u8);
            assert_eq!(after["P1"]["leader"]["life"], 20_u8);
            assert_eq!(after["P2"]["leader"]["life"], 20_u8);
            assert_eq!(after["P2"]["cemetery"], if evolved { json!(["low"]) } else { json!([]) });
            assert_eq!(after["P1"]["pp"]["current"], if evolved { 9_u8 } else { 10_u8 });
            if evolved {
                assert_eq!(after["objects"]["source"]["evolved_with"], "form");
                assert_eq!(after["objects"]["source"]["power"], 10_u8);
            }
            finish_combat(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if evolved { 10_u8 } else { 12_u8 },
                "the declared attack completes with the effective print's combat power"
            );
        }
    }
}

#[test]
fn bp04_118_attack_with_no_opposing_followers_still_completes() {
    for form in ["BP04-118", "BP04-SL23", "BP04-U07"] {
        let mut game = game(&position("BP04-117", form, false));
        evolve(&mut game);
        resolve_attack(&mut game, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["field"], json!(["amulet"]));
        assert_eq!(after["P2"]["ex"], json!(["ex-token"]));
        assert_eq!(after["P2"]["leader"]["life"], 20_u8);
        assert_eq!(after["objects"]["ally"]["damage"], 0_u8);
        finish_combat(&mut game);
        assert_eq!(game.projection(View::Referee).unwrap()["P2"]["leader"]["life"], 10_u8);
    }
}
