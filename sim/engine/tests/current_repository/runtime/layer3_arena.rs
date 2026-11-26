use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question};
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(number: &str, active: &str) -> Value {
    let mut value = draw_position();
    value["turn"]["active"] = json!(active);
    value["players"][active]["zones"]["hand"] = json!([{"id":"arena","card":number}]);
    for side in ["P1", "P2"] {
        value["players"][side]["zones"]["deck"] = json!([
            {"id":format!("{side}-dragon"),"card":"BP02-107"},
            {"id":format!("{side}-spell"),"card":"BP01-179"},
            {"id":format!("{side}-follower"),"card":"BP01-158"},
            {"id":format!("{side}-bottom"),"card":"BP01-179"}
        ]);
    }
    value
}

fn loaded(value: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "arena",
    )
    .unwrap()
}

fn choose_first(board: &mut Game, q: &DecisionView) -> CardPlay {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private follower choice");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the two looked followers are candidates"
    );
    board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
}

#[test]
fn arena_both_players_look_then_enter_then_mill_before_their_fanfares() {
    // Q872–873: the second chooser sees the first entry, and both remainders precede Fanfare.
    for (number, active, other, seat) in
        [("BP06-113", "P1", "P2", 1_u32), ("BP06-P31", "P2", "P1", 2)]
    {
        let mut board = loaded(&position(number, active));
        assert!(matches!(
            board
                .play_internal_card("arena", PlayerId { seat })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        let first = question(pending(&mut board, number));
        assert_eq!(first.actor, PlayerId { seat });
        assert_look_privacy(&board, None);
        let before_wrong_actor = board.projection(View::Referee).unwrap();
        board
            .answer_internal_card(
                PlayerId { seat: 3 - seat },
                first.token,
                Answer::Targets(vec![]),
            )
            .unwrap_err();
        assert_eq!(board.projection(View::Referee).unwrap(), before_wrong_actor);
        let second = question(choose_first(&mut board, &first));
        assert_eq!(second.actor.seat, 3 - seat);
        assert_look_privacy(&board, Some(active));
        let after_first_entry = board.projection(View::Referee).unwrap();
        assert_eq!(
            after_first_entry[active]["field"],
            json!(["arena", format!("{active}-dragon")])
        );
        assert_eq!(after_first_entry[other]["field"], json!([]));
        for side in ["P1", "P2"] {
            assert_eq!(after_first_entry[side]["cemetery"], json!([]));
        }
        board
            .answer_internal_card(first.actor, first.token, Answer::Targets(vec![]))
            .unwrap_err();
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            after_first_entry,
            "a previous player's token cannot answer the next choice"
        );
        resolved(choose_first(&mut board, &second));
        finish_checks(&mut board);
        let after_remainders = board.projection(View::Referee).unwrap();
        for side in ["P1", "P2"] {
            assert_eq!(
                after_remainders[side]["cemetery"],
                json!([format!("{side}-spell"), format!("{side}-follower")])
            );
            assert_eq!(
                after_remainders[side]["deck"],
                json!([format!("{side}-bottom")])
            );
        }
        let active_pending = board.internal_pending_choice().unwrap().unwrap();
        assert_eq!(
            active_pending.actor,
            PlayerId { seat },
            "turn player's Bahamut resolves first"
        );
        resolved(pending(&mut board, "BP02-107"));
        finish_checks(&mut board);
        let after_first_fanfare = board.projection(View::Referee).unwrap();
        assert_eq!(after_first_fanfare[other]["field"], json!([]));
        assert!(
            after_first_fanfare[active]["field"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("{active}-dragon")))
        );
        let other_pending = board.internal_pending_choice().unwrap().unwrap();
        assert_eq!(
            other_pending.actor.seat,
            3 - seat,
            "departed Bahamut's trigger survives"
        );
        resolved(pending(&mut board, "BP02-107"));
        finish_checks(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()[active]["field"],
            json!(["arena"])
        );
    }
}

fn assert_look_privacy(board: &Game, entered: Option<&str>) {
    for (side, foreign, view) in [("P1", "P2", View::P1), ("P2", "P1", View::P2)] {
        let private = board.projection(view).unwrap().to_string();
        for member in ["dragon", "spell", "follower"] {
            assert!(
                private.contains(&format!("\"{side}-{member}\"")),
                "both players retain their own looked cards"
            );
            assert_eq!(
                private.contains(&format!("\"{foreign}-{member}\"")),
                entered == Some(foreign) && member == "dragon",
                "only the entered card becomes public, including before the second choice"
            );
        }
        for owner in [side, foreign] {
            assert!(
                !private.contains(&format!("\"{owner}-bottom\"")),
                "the fourth card remains unobserved"
            );
        }
    }
}

#[test]
fn arena_entering_on_the_other_players_turn_keeps_its_controller_and_turn_order() {
    for (active, other, seat) in [("P1", "P2", 1_u32), ("P2", "P1", 2)] {
        let mut initial = position("BP06-113", active);
        initial["players"][active]["zones"]["hand"] = json!([]);
        initial["players"][active]["zones"]["field"] = json!([]);
        initial["players"][other]["zones"]["cemetery"] = json!([{"id":"arena","card":"BP06-113"}]);
        let opponents = PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        };
        // Only the external entry is synthetic; the complete Arena program stays selected.
        let mut board = with_producer(
            &initial,
            Action::Move {
                objects: Selector::Zone {
                    players: opponents.clone(),
                    zone: ZoneKind::Cemetery,
                    view: SelectionView::Current {},
                    binding: Binding::new(80, ScopeId(1)),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::RequireOne {
                                players: Box::new(opponents),
                            },
                            kind: ZoneKind::Field,
                        },
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        );
        activate(&mut board, active);
        finish_checks(&mut board);
        let pending_q = board.internal_pending_choice().unwrap().unwrap();
        assert_eq!(
            pending_q.actor.seat,
            3 - seat,
            "the non-turn player controls Arena's Fanfare"
        );
        let first = question(pending(&mut board, "BP06-113"));
        assert_eq!(
            first.actor,
            PlayerId { seat },
            "the turn player chooses first"
        );
        let second = question(
            board
                .answer_internal_card(first.actor, first.token, Answer::Targets(vec![]))
                .unwrap(),
        );
        assert_eq!(second.actor.seat, 3 - seat);
        resolved(
            board
                .answer_internal_card(second.actor, second.token, Answer::Targets(vec![]))
                .unwrap(),
        );
        finish_checks(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state[other]["field"], json!(["arena"]));
        for side in ["P1", "P2"] {
            assert_eq!(
                state[side]["cemetery"],
                json!([
                    format!("{side}-dragon"),
                    format!("{side}-spell"),
                    format!("{side}-follower")
                ])
            );
        }
    }
}

#[test]
fn arena_handles_short_decks_full_fields_and_declined_entries() {
    for (cards, full, decline) in [
        (0_u8, false, false),
        (1, false, false),
        (2, false, true),
        (3, true, false),
    ] {
        let mut initial = position("BP06-113", "P1");
        for side in ["P1", "P2"] {
            initial["players"][side]["zones"]["deck"] = json!((0..cards).map(|n| json!({"id":format!("{side}-d{n}"),"card":if n == 0 {"BP01-158"} else {"BP01-179"}})).collect::<Vec<_>>());
            initial["players"][side]["zones"]["field"] = json!(
                (0..if full {
                    if side == "P1" { 4_u8 } else { 5 }
                } else {
                    0
                })
                    .map(|n| json!({"id":format!("{side}-f{n}"),"card":"BP01-158"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut board = loaded(&initial);
        assert!(matches!(
            board
                .play_internal_card("arena", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        let mut play = pending(&mut board, "BP06-113");
        while let CardPlay::Awaiting(q) = play {
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("Look selection");
            };
            let targets = if decline || full || domain.members.is_empty() {
                vec![]
            } else {
                vec![domain.members[0].handle]
            };
            play = board
                .answer_internal_card(q.actor, q.token, Answer::Targets(targets))
                .unwrap();
        }
        resolved(play);
        finish_checks(&mut board);
        let state = board.projection(View::Referee).unwrap();
        let enters = cards > 0 && !full && !decline;
        for side in ["P1", "P2"] {
            assert_eq!(state[side]["deck"], json!([]));
            let cemetery: Vec<_> = (u8::from(enters)..cards)
                .map(|n| format!("{side}-d{n}"))
                .collect();
            assert_eq!(state[side]["cemetery"], json!(cemetery));
            assert_eq!(
                state[side]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("{side}-d0"))),
                enters
            );
        }
    }
}

#[test]
fn arena_printed_restriction_checks_legal_followers_without_requiring_an_attack() {
    for (active, other) in [("P1", "P2"), ("P2", "P1")] {
        for acted in [false, true] {
            let mut initial = position("BP06-113", active);
            initial["players"][active]["zones"]["hand"] = json!([]);
            initial["players"][active]["zones"]["field"] =
                json!([{"id":"attacker","card":"BP01-158"}]);
            initial["players"][other]["zones"]["field"] = json!([
                {"id":"arena","card":"BP06-113"}, {"id":"enemy","card":"BP01-158","state":{"acted":acted}}
            ]);
            let mut board = loaded(&initial);
            assert_eq!(board.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":format!("{other}.leader")})).unwrap().outcome, if acted {"cannot-attack"} else {"resolved"});
            if acted {
                assert_eq!(
                    board
                        .internal_rule_action(&json!({"do":"end-phase"}))
                        .unwrap()
                        .outcome,
                    "resolved"
                );
            }
        }
    }
}
