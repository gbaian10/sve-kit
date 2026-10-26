#![expect(
    clippy::panic,
    reason = "An unexpected pending domain means the scenario did not reach the Fanfare."
)]

use super::*;

fn position(seat: &str, number: &str, allies: bool) -> Value {
    let mut initial = draw_position();
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"commander","card":number}]);
    initial["players"][seat]["zones"]["field"] = if allies {
        json!([
            {"id":"ally-1","card":"BP01-037"},
            {"id":"ally-2","card":"BP01-037"}
        ])
    } else {
        json!([])
    };
    initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn play_fanfare(game: &mut Game, seat: &str, number: &str) {
    let entry = game
        .play_internal_card(
            "commander",
            PlayerId {
                seat: seat_number(seat),
            },
        )
        .unwrap();
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "commander enters the field"
    );
    assert!(
        game.internal_decision(View::Referee).is_none(),
        "entry needs no targets"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(
        question.actor.seat,
        seat_number(seat),
        "controller chooses the pending ability"
    );
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending Fanfare");
    };
    let [option] = options.as_slice() else {
        panic!("one Fanfare");
    };
    assert_eq!(
        option.card, number,
        "pending ability identifies the played card"
    );
    assert_eq!(option.line, 1, "Fanfare keeps its printed line");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "no target selection or resolution choice"
    );
    assert!(
        game.internal_decision(View::Referee).is_none(),
        "buff resolves without a choice"
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "one Fanfare resolves once"
    );
}

fn assert_stats(game: &Game, allies: bool) {
    for view in [View::P1, View::P2, View::Referee] {
        let state = game.projection(view).unwrap();
        for id in ["commander", "enemy"] {
            assert_eq!(
                state["objects"][id]["power"], 5_u8,
                "source and opponent keep printed attack"
            );
            assert_eq!(
                state["objects"][id]["hp"], 5_u8,
                "source and opponent keep printed life"
            );
        }
        if allies {
            for id in ["ally-1", "ally-2"] {
                assert_eq!(
                    state["objects"][id]["power"], 6_u8,
                    "each other allied follower gains one attack"
                );
                assert_eq!(
                    state["objects"][id]["hp"], 6_u8,
                    "each other allied follower gains one life"
                );
            }
        }
    }
}

#[test]
fn bp01_037_fanfare_buffs_other_allies_without_targets() {
    for seat in ["P1", "P2"] {
        let mut game = game(&position(seat, "BP01-037", true));
        play_fanfare(&mut game, seat, "BP01-037");
        assert_stats(&game, true);
    }
}

#[test]
fn bp01_037_fanfare_with_no_other_allies_leaves_self_and_opponent_unchanged() {
    for seat in ["P1", "P2"] {
        let mut game = game(&position(seat, "BP01-037", false));
        play_fanfare(&mut game, seat, "BP01-037");
        assert_stats(&game, false);
    }
}

#[test]
fn bp01_037_buffs_remain_after_commander_leaves_the_field() {
    for seat in ["P1", "P2"] {
        let mut initial = position(seat, "BP01-037", true);
        for ally in initial["players"][seat]["zones"]["field"]
            .as_array_mut()
            .unwrap()
        {
            ally["card"] = json!(NUMBER);
        }
        let binding = Binding::new(50, ScopeId(1));
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        text_quantifier: TextQuantifier::Implicit,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(0),
                            maximum: None,
                        },
                    },
                    zone: ZoneKind::Field,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(Expr::Compute(Box::new(Predicate::HasTrait {
                        traits: Expr::Live(Box::new(TraitsRead::Traits {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::Bound { binding }),
                            },
                            view: CharacteristicView::Effective,
                        })),
                        trait_id: TraitId("指揮官".into()),
                    }))),
                },
            },
        );
        let before = game.projection(View::Referee).unwrap();
        play_fanfare(&mut game, seat, "BP01-037");
        let buffed = game.projection(View::Referee).unwrap();
        for id in ["ally-1", "ally-2"] {
            for stat in ["power", "hp"] {
                assert_eq!(
                    buffed["objects"][id][stat].as_i64().unwrap(),
                    before["objects"][id][stat].as_i64().unwrap() + 1
                );
            }
        }
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after[seat]["cemetery"], json!(["commander"]));
        for id in ["ally-1", "ally-2"] {
            for stat in ["power", "hp"] {
                assert_eq!(after["objects"][id][stat], buffed["objects"][id][stat]);
            }
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
