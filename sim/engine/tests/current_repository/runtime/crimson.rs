use super::*;

#[test]
fn bp01_117_fanfare_requires_own_leader_to_have_lost_life_this_turn() {
    for seat in ["P1", "P2"] {
        for (own, delta, expected) in [
            (true, -1_i64, 3_i64),
            (true, 0, 2),
            (true, 1, 2),
            (false, -1, 2),
        ] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] = json!([]);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"fighter","card":"BP01-117"}]);
            let mut game = with_producer(
                &initial,
                Action::Life {
                    player: if own {
                        PlayerRef::Controller {}
                    } else {
                        PlayerRef::RequireOne {
                            players: Box::new(PlayerSet {
                                relation: PlayerRelation::Opponents,
                                relative_to: PlayerRef::Controller {},
                                cardinality: Cardinality {
                                    minimum: Nat(1),
                                    maximum: Some(Nat(1)),
                                },
                                text_quantifier: TextQuantifier::Implicit,
                            }),
                        }
                    },
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(delta)),
                    },
                },
            );
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            assert!(matches!(
                game.play_internal_card(
                    "fighter",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let question = game.internal_pending_choice().unwrap().unwrap();
            if let InputDomain::Pending(options) = &question.domain {
                assert_eq!(options.len(), 1);
                assert_eq!(options[0].card, "BP01-117");
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
                );
            } else {
                panic!("expected fanfare");
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["fighter"]["power"], expected);
            assert_eq!(state["objects"]["fighter"]["hp"], expected);
        }
    }
}
