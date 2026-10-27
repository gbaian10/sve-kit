#![expect(
    clippy::panic,
    reason = "A different pending domain invalidates the Fanfare scenario."
)]

use super::*;
use sve_engine::game::internal::{InstructionOutcome, ResourceOutcome};

fn resolve_ep(game: &mut Game, seat: &str, before_ep: i64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending Fanfare");
    };
    let [option] = options.as_slice() else {
        panic!("exactly one printed Fanfare");
    };
    assert_eq!(
        option.card, "BP03-067",
        "the entered card owns the pending ability"
    );
    assert_eq!(option.line, 1_u64, "the single printed Fanfare is retained");
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: if question.actor.seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "wrong actor preserves the pending entry"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap(),
    );
    let resources = result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Resource(resource) = outcome {
                Some(resource)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(resources.len(), 1, "only one EP instruction completes");
    assert_eq!(
        *resources[0],
        ResourceOutcome {
            player: PlayerId {
                seat: seat_number(seat)
            },
            resource: Resource::Ep,
            requested: Int(1),
            before: Int(before_ep),
            after: Int(before_ep.saturating_add(1)),
            actual: Int(1),
        },
        "EP completes independently of the PP entry payment"
    );
    let completed = format!("{game:?}");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == completed,
        "old token cannot grant a second EP"
    );
}

#[test]
fn bp03_067_adds_ep_after_payment_and_only_buffs_at_maximum_pp_seven() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for maximum in [6_u8, 7] {
            for before_ep in [0_i64, 2] {
                let mut initial = draw_position();
                initial["turn"]["active"] = json!(seat);
                initial["players"][seat]["pp"] = json!({"current":maximum,"max":maximum});
                initial["players"][seat]["ep"] = json!(before_ep);
                initial["players"][opponent]["ep"] = json!(1_i64);
                initial["players"][seat]["zones"]["hand"] =
                    json!([{"id":"elder","card":"BP03-067"}]);
                let mut game = game(&initial);
                assert!(matches!(
                    game.play_internal_card(
                        "elder",
                        PlayerId {
                            seat: seat_number(seat)
                        }
                    )
                    .unwrap(),
                    CardPlay::Entered(_)
                ));
                let entered = game.projection(View::Referee).unwrap();
                assert_eq!(
                    entered[seat]["ep"], before_ep,
                    "entry itself does not grant EP"
                );
                assert_eq!(entered[seat]["pp"]["current"], maximum.saturating_sub(4));
                resolve_ep(&mut game, seat, before_ep);
                let (power, hp) = if maximum == 7 {
                    (5_u8, 7_u8)
                } else {
                    (3_u8, 5_u8)
                };
                for view in [View::P1, View::P2, View::Referee] {
                    let state = game.projection(view).unwrap();
                    assert_eq!(state[seat]["ep"], before_ep.saturating_add(1));
                    assert_eq!(state[opponent]["ep"], 1_i64);
                    assert_eq!(state[seat]["sep"], 0_u8);
                    assert_eq!(
                        state[seat]["pp"],
                        json!({"current":maximum.saturating_sub(4),"max":maximum})
                    );
                    assert_eq!(state["objects"]["elder"]["power"], power);
                    assert_eq!(state["objects"]["elder"]["hp"], hp);
                }
            }
        }
    }
}
