use super::event_cemetery::finish_checks;
use super::event_evolved::{from_position, position};
use super::*;

fn evolved_at_fifteen(card: &str, side: &str) -> Game {
    let mut initial = position(card, side);
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    let mut game = from_position(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"subject","evolve_card":"form","pay":{"pp":3_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real Ward-only subject evolves for its printed three PP"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()[side]["pp"]["current"],
        7_u8,
        "the native evolution actually pays its PP"
    );
    finish_checks(&mut game);
    game
}

#[expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the native evolution choice."
)]
fn resolve_choice(game: &mut Game, card: &str, damages: bool) {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("native evolution observer")
    };
    assert_eq!(
        options.len(),
        1,
        "only the real observer responds to the Ward-only evolution"
    );
    assert_eq!(
        options[0].card, card,
        "the printed observer owns this occurrence"
    );
    let step = game
        .answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(choice) = step else {
        panic!("native mode declaration")
    };
    assert!(
        matches!(&choice.domain, InputDomain::Modes { .. }),
        "the printed choice precedes any target"
    );
    let play = game
        .answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Modes(vec![if damages { ModeId(1) } else { ModeId(2) }]),
        )
        .unwrap();
    if damages {
        let CardPlay::Awaiting(declaration) = play else {
            panic!("damage mode's formal enemy target")
        };
        let InputDomain::Targets(domain) = &declaration.domain else {
            panic!("formal opposing follower domain")
        };
        let enemy = domain
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some("evolve-enemy"))
            .unwrap();
        resolved(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Targets(vec![enemy.handle]),
            )
            .unwrap(),
        );
    } else {
        resolved(play);
    }
    finish_checks(game);
}

#[test]
fn bp09_112_printings_choose_native_damage_with_a_formal_target_or_heal_without_one() {
    for card in ["BP09-112", "PR-237"] {
        for damages in [false, true] {
            let mut game = evolved_at_fifteen(card, "P1");
            resolve_choice(&mut game, card, damages);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["enemy"]["damage"],
                if damages { 2_u8 } else { 0_u8 },
                "only the declared damage mode damages the selected enemy"
            );
            assert_eq!(
                after["P1"]["leader"]["life"],
                if damages { 15_u8 } else { 16_u8 },
                "only the untargeted healing mode restores life"
            );
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "the one evolution occurrence finishes once"
            );
        }
    }
}

#[test]
fn bp09_112_printings_do_not_offer_a_choice_for_opposing_native_evolution() {
    for card in ["BP09-112", "PR-237"] {
        let mut game = evolved_at_fifteen(card, "P2");
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the opponent's evolution never subscribes this observer"
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["leader"]["life"], 15_u8,
            "no opposing evolution restores allied life"
        );
        assert_eq!(
            after["objects"]["enemy"]["damage"], 0_u8,
            "no opposing evolution deals observer damage"
        );
    }
}
