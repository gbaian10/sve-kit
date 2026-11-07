#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed movement scenarios."
)]
use super::event_cemetery::{finish_checks, move_named};
use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|n| json!({"id":format!("d-{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    let handle = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn finish(board: &mut Game, mut play: CardPlay, accept: bool, target: Option<&str>) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                let count = usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap();
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .filter(|m| target.is_none_or(|number| m.card.as_deref() == Some(number)))
                        .take(count)
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            InputDomain::Integer(bounds) => {
                assert_eq!(bounds.minimum, Nat(0), "optional movement lower bound");
                assert_eq!(
                    bounds.maximum,
                    Some(Nat(1)),
                    "optional movement upper bound"
                );
                Answer::Integer(Nat(u64::from(accept)))
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("only the printed choice, optional movement, order or Ward entry"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "complete printed ability"
    );
}

fn enter_follower(board: &mut Game) {
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its pending printed abilities resolve"
    );
}

#[test]
fn angelic_vanguard_all_printings_banish_a_field_follower_only_at_five_angels() {
    for number in ["BP09-106", "PR-233"] {
        for count in [4_u8, 5] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("a-{n}"),"card":"BP01-159"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut board = game(&initial);
            enter_follower(&mut board);
            let play = trigger(&mut board, number, 2);
            finish(&mut board, play, true, None);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["banish"],
                if count == 5 {
                    json!(["enemy"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn nitro_juggler_all_printings_banish_two_looked_cards_reorder_rest_and_gain_keywords() {
    for number in ["CP03-053", "CP03-P36"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0..3_u8)
                .map(|n| json!({"id":format!("b-{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        enter_follower(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["keywords"],
            json!([])
        );
        let play = trigger(&mut board, number, 2);
        finish(&mut board, play, true, None);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["banish"],
            json!(["b-0", "b-1", "b-2", "d-0", "d-1"])
        );
        assert_eq!(view["P1"]["deck"], json!(["d-5", "d-4", "d-3", "d-2"]));
        let keywords = view["objects"]["source"]["keywords"].as_array().unwrap();
        assert!(keywords.contains(&json!("必殺")) && keywords.contains(&json!("守護")));
    }
}

fn ilganno_position(number: &str, grave: u8, full: bool) -> Value {
    let mut initial = position(number);
    let mut cards = vec![json!({"id":"ally","card":"BP11-065"})];
    cards.extend((1..grave).map(|n| json!({"id":format!("g-{n}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards);
    if full {
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..4_u8)
                .map(|n| json!({"id":format!("f-{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

#[test]
fn ilganno_all_printings_banish_self_only_after_accepted_successful_necro_entry() {
    for number in ["BP11-071", "BP11-SL15", "EBD03-007", "PR-399"] {
        for (grave, accept, full) in [
            (9_u8, true, false),
            (10, false, false),
            (10, true, false),
            (10, true, true),
        ] {
            let mut board = game(&ilganno_position(number, grave, full));
            enter_follower(&mut board);
            let play = trigger(&mut board, number, 1);
            finish(&mut board, play, accept, Some("BP11-065"));
            let view = board.projection(View::Referee).unwrap();
            let entered = grave >= 10 && accept && !full;
            assert_eq!(
                view["P1"]["banish"],
                if entered {
                    json!(["source"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("ally")),
                entered
            );
            assert_eq!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("ally")),
                !entered
            );
        }
    }
}

#[test]
fn ilganno_departed_source_and_reentered_self_do_not_follow_old_generation() {
    for number in ["BP11-071", "BP11-SL15", "EBD03-007", "PR-399"] {
        for revive_self in [false, true] {
            let mut initial = ilganno_position(number, 10, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"source","card":number}));
            let mut enter = move_named(
                ZoneKind::Cemetery,
                OwnerZone::Hand,
                "ストレイホロウ・イルガンノ",
            );
            let Action::Move { movement, .. } = &mut enter else {
                panic!("entry movement");
            };
            movement.to = MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            };
            let objects = Selector::MoveAfter {
                receipt: MovementReceipt::Move {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            };
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"producer","card":"producer"}]);
            let mut board = Game::from_selected_effects(
                producer_selection().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "banish-events",
            )
            .unwrap();
            board
                .load_internal_card(
                    "producer",
                    &producer_steps(vec![enter, Action::Destroy { objects }]),
                )
                .unwrap();
            activate(&mut board, "P1");
            let fanfare = trigger(&mut board, number, 1);
            finish(
                &mut board,
                fanfare,
                true,
                Some(if revive_self { number } else { "BP11-065" }),
            );
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["banish"], json!([]));
            assert_eq!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                revive_self
            );
            let lastword = trigger(&mut board, number, 2);
            finish(&mut board, lastword, true, None);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(
                after["P1"]["ex"],
                if revive_self {
                    json!([])
                } else {
                    json!(["source"])
                }
            );
            assert!(
                after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("d-0")),
                "Lastword still mills"
            );
        }
    }
}
