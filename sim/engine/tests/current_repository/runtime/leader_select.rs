use super::*;

#[test]
fn bp01_094_fanfare_selects_a_leader_or_follower_after_entry() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for enemy in [false, true] {
            for leader in [false, true] {
                if !enemy && !leader {
                    continue;
                }
                let mut initial = draw_position();
                initial["turn"]["active"] = json!(seat);
                initial["players"][seat]["zones"]["hand"] =
                    json!([{"id":"archer","card":"BP01-094"}]);
                if enemy {
                    initial["players"][opponent]["zones"]["field"] = json!([
                        {"id":"enemy","card":"BP01-037"},
                        {"id":"amulet","card":"BP19-108"}
                    ]);
                }
                let mut game = game(&initial);
                assert!(matches!(
                    game.play_internal_card(
                        "archer",
                        PlayerId {
                            seat: seat_number(seat)
                        }
                    )
                    .unwrap(),
                    CardPlay::Entered(_)
                ));
                assert!(game.internal_decision(View::Referee).is_none());
                let pending = game.internal_pending_choice().unwrap().unwrap();
                let InputDomain::Pending(options) = &pending.domain else {
                    panic!("Fanfare")
                };
                let CardPlay::Awaiting(question) = game
                    .answer_internal_card(
                        pending.actor,
                        pending.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap()
                else {
                    panic!("resolution recipient choice")
                };
                let InputDomain::Targets(domain) = &question.domain else {
                    panic!("targets")
                };
                assert_eq!(domain.members.len(), 1 + usize::from(enemy));
                assert_eq!(domain.cardinality.minimum, Nat(1));
                assert!(
                    domain
                        .members
                        .iter()
                        .all(|member| member.player.seat == seat_number(opponent))
                );
                let chosen = domain
                    .members
                    .iter()
                    .find(|member| member.card.is_none() == leader)
                    .unwrap()
                    .handle;
                let before = format!("{game:?}");
                for handles in [vec![], vec![99], vec![chosen, chosen]] {
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(handles),
                    )
                    .unwrap_err();
                    assert!(
                        format!("{game:?}") == before,
                        "invalid answers preserve entry and the choice"
                    );
                }
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![chosen]),
                    )
                    .unwrap(),
                );
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state[opponent]["leader"]["life"],
                    if leader { 19_u8 } else { 20_u8 }
                );
                assert_eq!(state[seat]["leader"]["life"], 20_u8);
                if enemy {
                    assert_eq!(state["objects"]["enemy"]["damage"], u8::from(!leader));
                }
                assert_eq!(state[seat]["field"], json!(["archer"]));
            }
        }
    }
}
