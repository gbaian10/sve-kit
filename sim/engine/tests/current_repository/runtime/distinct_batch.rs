#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::distinct_count::{fixture, position};
use super::*;

fn trigger(game: &mut Game) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn resolve_target(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(question) = play else {
        panic!("mandatory target")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "Only the enemy follower is a legal target"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP04-117"),
        "The declared target keeps its printed identity"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
}

#[test]
fn bp22_048_requires_its_formal_target_even_when_all_three_conditional_rewards_are_available() {
    for printing in ["BP22-048", "BP22-P25"] {
        for count in [5_usize, 6] {
            for available in [false, true] {
                let mut initial = position(printing, count);
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"BP04-117"}]);
                if !available {
                    initial["players"]["P2"]["zones"]["field"] = json!([]);
                }
                let mut game = fixture(&initial);
                assert!(matches!(
                    game.play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ));
                let play = trigger(&mut game);
                if available {
                    resolve_target(&mut game, play);
                } else {
                    assert_eq!(play, CardPlay::Cancelled);
                }
                let rewarded = available && count == 6;
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["leader"]["life"],
                    if rewarded { 13_u8 } else { 10_u8 }
                );
                assert_eq!(
                    state["P1"]["pp"]["current"],
                    if rewarded { 9_u8 } else { 6_u8 }
                );
                assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
                if available {
                    assert_eq!(
                        state["objects"]["enemy"]["damage"],
                        if rewarded { 3_u8 } else { 0_u8 }
                    );
                }
            }
        }
    }
}

#[test]
fn bp22_050_original_cost_discount_applies_to_play_but_not_evolution_and_fanfare_still_damages_two()
{
    for printing in ["BP22-050", "BP22-P27"] {
        for count in [5_usize, 6] {
            let mut initial = position(printing, count);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP22-051"}]);
            let mut game = fixture(&initial);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let play = trigger(&mut game);
            resolve_target(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if count == 6 { 10_u8 } else { 4_u8 }
            );
            assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
            game.resume_internal_card().unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if count == 6 { 7_u8 } else { 1_u8 }
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
        }
    }
}
