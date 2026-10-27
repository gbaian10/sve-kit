#![expect(
    clippy::panic,
    reason = "A missing question fails the scenario immediately."
)]

use super::*;
use sve_engine::game::internal::{DamageShare, DecisionView};

fn opposite(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}

#[test]
fn bp01_099_uses_maximum_pp_and_hits_both_fields_at_the_threshold() {
    for seat in ["P1", "P2"] {
        for (maximum, damage) in [(6_u8, 2_u8), (7, 3)] {
            let opponent = opposite(seat);
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-099"}]);
            initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
            initial["players"][opponent]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = game(&initial);
            resolved(
                game.play_internal_card(
                    "spell",
                    PlayerId {
                        seat: seat_number(seat),
                    },
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            for id in ["ally", "enemy"] {
                assert_eq!(state["objects"][id]["damage"], damage);
            }
            assert_eq!(state[seat]["pp"]["current"], maximum - 3);
            assert_eq!(state[opponent]["leader"]["life"], 20_u8);
        }
    }
}

#[test]
fn bp08_028_uses_opponent_cemetery_for_distribution_and_the_later_condition() {
    for seat in ["P1", "P2"] {
        for count in [0_u64, 9, 10, 19, 20] {
            let opponent = opposite(seat);
            let (total, share) = if count >= 10 { (8_u64, 4_u64) } else { (4, 2) };
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP08-028"}]);
            initial["players"][opponent]["zones"]["field"] = json!([
                {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"}
            ]);
            initial["players"][opponent]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("grave{n}"),"card":"CP02-068"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            let question = awaiting(
                game.play_internal_card(
                    "spell",
                    PlayerId {
                        seat: seat_number(seat),
                    },
                )
                .unwrap(),
            );
            let distribution = awaiting(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![0, 1]),
                )
                .unwrap(),
            );
            assert!(
                matches!(&distribution.domain, InputDomain::Distribution(domain) if domain.total == Nat(total))
            );
            let before = format!("{game:?}");
            game.answer_internal_card(
                distribution.actor,
                distribution.token,
                Answer::Distribution(vec![
                    DamageShare {
                        handle: 0,
                        amount: Nat(total - 1),
                    },
                    DamageShare {
                        handle: 1,
                        amount: Nat(0),
                    },
                ]),
            )
            .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "invalid allocation is atomic"
            );
            resolved(
                game.answer_internal_card(
                    distribution.actor,
                    distribution.token,
                    Answer::Distribution(vec![
                        DamageShare {
                            handle: 0,
                            amount: Nat(share),
                        },
                        DamageShare {
                            handle: 1,
                            amount: Nat(share),
                        },
                    ]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["first"]["damage"], share);
            assert_eq!(state["objects"]["second"]["damage"], share);
            assert_eq!(
                state[opponent]["leader"]["life"],
                if count >= 20 { 12_u8 } else { 20 }
            );
        }
    }
}

#[test]
fn bp08_028_accepts_zero_targets_without_requiring_a_damage_allocation() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP08-028"}]);
    let mut game = game(&initial);
    let question = awaiting(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    let next = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap();
    match next {
        CardPlay::Resolved(_) => {}
        CardPlay::Awaiting(distribution) => {
            assert!(
                matches!(&distribution.domain, InputDomain::Distribution(domain) if domain.members.is_empty())
            );
            resolved(
                game.answer_internal_card(
                    distribution.actor,
                    distribution.token,
                    Answer::Distribution(vec![]),
                )
                .unwrap(),
            );
        }
        CardPlay::Entered(_) | CardPlay::Cancelled | CardPlay::Continued(_) => {
            panic!("unexpected spell continuation")
        }
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
}

#[test]
fn bp20_r15_nested_hand_counts_only_damage_at_its_own_end() {
    for seat in ["P1", "P2"] {
        for count in [0_u64, 1, 2, 3] {
            let opponent = opposite(seat);
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"watcher","card":"BP20-R15"}]);
            initial["players"][opponent]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037"}]);
            initial["players"][seat]["zones"]["hand"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("hand{n}"),"card":"CP02-068"}))
                    .collect::<Vec<_>>()
            );
            let mut wrong_turn = initial.clone();
            wrong_turn["turn"]["active"] = json!(opponent);
            let mut other = game(&wrong_turn);
            other
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            assert!(other.internal_pending_choice().unwrap().is_none());
            let mut game = game(&initial);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("end trigger")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP20-R15");
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[opponent]["leader"]["life"],
                if count <= 2 { 17_u8 } else { 20 }
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 0 { 3_u8 } else { 0 }
            );
        }
    }
}

#[test]
fn sd07_016_quick_activation_pays_act_and_cannot_repeat_while_acted() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"source","card":"SD07-016"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("quick window")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "SD07-016");
    assert_eq!(options[0].line, Some(1));
    let before = format!("{game:?}");
    game.activate_internal_card("source", AbilityId(1), question.actor)
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "Quick activation must use the window handle"
    );
    let target = awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::Quick(Some(0)))
            .unwrap(),
    );
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
    game.resume_internal_card().unwrap();
    let next = game.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(&next.domain, InputDomain::Quick { options: remaining, .. } if remaining.is_empty())
    );
}
