use super::*;

fn position(seat: &str, opponent: &str, available: bool) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"raiser","card":"BP01-124"}]);
    let mut grave = vec![json!({"id":"spell","card":"CP02-068"})];
    if available {
        grave.extend(["first", "second", "third"].map(|id| json!({"id":id,"card":"BP01-037"})));
    }
    initial["players"][seat]["zones"]["cemetery"] = json!(grave);
    initial["players"][opponent]["zones"]["cemetery"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

#[test]
fn bp01_124_chooses_only_own_grave_followers_and_hides_the_returned_hand_slots() {
    for seat in ["P1", "P2"] {
        for available in [false, true] {
            let opponent = if seat == "P1" { "P2" } else { "P1" };
            let initial = position(seat, opponent, available);
            let mut game = game(&initial);
            assert!(matches!(
                game.play_internal_card(
                    "raiser",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("expected fanfare")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP01-124");
            let CardPlay::Awaiting(question) = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("expected cemetery choice")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected public targets")
            };
            assert_eq!(domain.members.len(), usize::from(available) * 3);
            assert_eq!(domain.cardinality.minimum, Nat(0));
            assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
            let before = format!("{game:?}");
            game.answer_internal_card(
                PlayerId {
                    seat: seat_number(opponent),
                },
                question.token,
                Answer::Targets(vec![]),
            )
            .unwrap_err();
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(if available { vec![0, 0] } else { vec![0] }),
            )
            .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "wrong actor and invalid handles leave the choice unchanged"
            );
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(if available { vec![0, 1] } else { vec![] }),
                )
                .unwrap(),
            );
            let moves = result.progress.facts.iter().filter(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Put && m.from.kind == ZoneKind::Cemetery)).count();
            assert_eq!(moves, usize::from(available) * 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["hand"],
                if available {
                    json!(["first", "second"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state[seat]["cemetery"],
                if available {
                    json!(["spell", "third"])
                } else {
                    json!(["spell"])
                }
            );
            assert_eq!(state[opponent]["cemetery"], json!(["enemy"]));
            let hidden = game
                .projection(if seat == "P1" { View::P2 } else { View::P1 })
                .unwrap();
            assert!(hidden["objects"].get("first").is_none());
            assert!(hidden["objects"].get("second").is_none());
            assert_eq!(
                projected_zone_count(&hidden[seat]["hand"]),
                u64::from(available) * 2
            );
        }
    }
}
