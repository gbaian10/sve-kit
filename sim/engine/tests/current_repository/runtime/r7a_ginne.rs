#![expect(
    clippy::panic,
    reason = "Printed mode and private-hand choices must have the expected domain."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::{DecisionView, TargetRoleAssignment};

fn position(number: &str, acted: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":number},{"id":"old","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":acted}}
    ]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw"),"card":"BP01-171"},
            {"id":format!("{seat}-middle"),"card":"BP01-037"},
            {"id":format!("{seat}-tail"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed decision");
    };
    question
}

fn fanfare(board: &mut Game, number: &str) -> DecisionView {
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the printed follower enters normally"
    );
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed Fanfare");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    awaiting(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(option.handle),
            )
            .unwrap(),
    )
}

fn choose_modes(board: &mut Game, question: &DecisionView, modes: &[u64]) -> CardPlay {
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(modes.iter().map(|&mode| ModeId(mode)).collect()),
        )
        .unwrap()
}

fn return_hand(board: &mut Game, play: CardPlay, top: bool) {
    let question = awaiting(play);
    let InputDomain::Targets(hand) = &question.domain else {
        panic!("private hand after drawing");
    };
    assert_eq!(
        hand.members.len(),
        2,
        "both the original and newly drawn card can be returned"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect private candidates"
    );
    let after_draw = board.projection(View::Referee).unwrap();
    assert_eq!(
        after_draw["P1"]["hand"],
        json!(["old", "P1-draw"]),
        "the printed draw happens before hand selection"
    );
    assert_eq!(
        after_draw["objects"]["enemy"]["acted"], true,
        "mode 1 resolves before mode 2 even when answered in reverse order"
    );
    assert_eq!(
        after_draw["P1"]["ex"],
        json!([]),
        "mode 3 cannot create Gold Coins before mode 2's private return completes"
    );
    let chosen = hand
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap();
    assert_eq!(
        hand.roles
            .iter()
            .map(|role| role.position)
            .collect::<Vec<_>>(),
        vec![Some(Position::Top {}), Some(Position::Bottom {})],
        "the same private question advertises the two deck edges"
    );
    let roles = hand
        .roles
        .iter()
        .map(|role| TargetRoleAssignment {
            role: role.role,
            targets: if role.position
                == Some(if top {
                    Position::Top {}
                } else {
                    Position::Bottom {}
                }) {
                vec![chosen.handle]
            } else {
                vec![]
            },
        })
        .collect();
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::TargetsWithRoles {
                    targets: vec![chosen.handle],
                    roles,
                },
            )
            .unwrap(),
    );
    finish_checks(board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["hand"],
        json!(["P1-draw"]),
        "only the selected old card leaves hand"
    );
    assert_eq!(
        after["P1"]["deck"],
        if top {
            json!(["old", "P1-middle", "P1-tail"])
        } else {
            json!(["P1-middle", "P1-tail", "old"])
        },
        "the same physical card reaches the chosen deck edge"
    );
    assert!(
        board.projection(View::P2).unwrap()["objects"]["old"].is_null(),
        "the return does not reveal a hidden card or tracked deck slot"
    );
}

fn target_before_draw(board: &mut Game, play: CardPlay) -> CardPlay {
    let target = awaiting(play);
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("formal enemy before drawing");
    };
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["hand"],
        json!(["old"]),
        "Q2087 prohibits seeing the draw before target declaration"
    );
    assert_eq!(
        before["P1"]["deck"][0], "P1-draw",
        "the deck top has not moved during formal target choice"
    );
    let enemy = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap();
    board
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![enemy.handle]),
        )
        .unwrap()
}

fn end_turn(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish_checks(board);
}

#[test]
fn r7a_ginne_all_printings_choose_targets_before_draw_and_return_a_private_card_to_either_edge() {
    for number in ["BP16-022", "BP16-SL08", "PR-449"] {
        for (top, acted) in [(true, false), (false, true)] {
            let mut board = game(&position(number, acted));
            let modes = fanfare(&mut board, number);
            let InputDomain::Modes { cardinality, .. } = &modes.domain else {
                panic!("printed modes");
            };
            assert_eq!(
                cardinality.maximum,
                Some(Nat(2)),
                "without Jiemon at most two modes are offered"
            );
            let play = choose_modes(&mut board, &modes, &[2, 1]);
            let play = target_before_draw(&mut board, play);
            return_hand(&mut board, play, top);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["ex"],
                json!([]),
                "unchosen Gold Coin mode does not run"
            );
            if number == "BP16-022" {
                end_turn(&mut board);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
                    true,
                    "Q2086: even an already acted target skips its next ordinary Start stand"
                );
                end_turn(&mut board);
                end_turn(&mut board);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
                    false,
                    "the stand prohibition expires after that opponent Start"
                );
            }
        }
    }
}

#[test]
fn r7a_ginne_missing_enemy_and_duplicate_modes_are_rejected_before_gold_creation() {
    let mut initial = position("BP16-022", false);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = game(&initial);
    let modes = fanfare(&mut board, "BP16-022");
    let InputDomain::Modes {
        modes: available, ..
    } = &modes.domain
    else {
        panic!("printed modes");
    };
    let InputDomain::Modes { cardinality, .. } = &modes.domain else {
        panic!("mode bounds");
    };
    assert_eq!(
        cardinality.minimum,
        Nat(1),
        "CR5.18.2.1 requires at least one printed mode"
    );
    assert!(
        !available.contains(&ModeId(1)),
        "Q2085: an unavailable formal target excludes mode 1"
    );
    let before = format!("{board:?}");
    for selected in [
        vec![],
        vec![ModeId(1)],
        vec![ModeId(3), ModeId(3)],
        vec![ModeId(1), ModeId(2), ModeId(3)],
    ] {
        board
            .answer_internal_card(modes.actor, modes.token, Answer::Modes(selected))
            .unwrap_err();
        assert_eq!(
            format!("{board:?}"),
            before,
            "illegal modes preserve the complete authority"
        );
    }
    resolved(choose_modes(&mut board, &modes, &[3]));
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["ex"].as_array().unwrap().len(),
        2,
        "the sole chosen mode creates two admitted Gold Coins"
    );
    assert!(
        after["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| after["objects"][id.as_str().unwrap()]["card"] == "BP14-T02"),
        "the printed token dependency is used"
    );
    assert_eq!(
        after["P1"]["hand"],
        json!(["old"]),
        "mode 2 was not chosen so there is no draw"
    );
}

#[test]
fn r7a_ginne_only_own_field_jiemon_allows_three_modes_and_resolution_stays_in_printed_order() {
    for (seat, zone, card, max) in [
        ("P1", "field", "BP14-022", 3_u64),
        ("P2", "field", "BP14-022", 2),
        ("P1", "ex", "BP14-022", 2),
        ("P1", "field", "BP01-037", 2),
    ] {
        let mut initial = position("BP16-022", false);
        if initial["players"][seat]["zones"][zone].is_null() {
            initial["players"][seat]["zones"][zone] = json!([]);
        }
        initial["players"][seat]["zones"][zone]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"presence","card":card}));
        let mut board = game(&initial);
        let modes = fanfare(&mut board, "BP16-022");
        let InputDomain::Modes { cardinality, .. } = &modes.domain else {
            panic!("printed modes");
        };
        assert_eq!(
            cardinality.maximum,
            Some(Nat(max)),
            "only the named ally on the own field changes the printed maximum"
        );
        if max == 3 {
            let play = choose_modes(&mut board, &modes, &[3, 2, 1]);
            let play = target_before_draw(&mut board, play);
            return_hand(&mut board, play, false);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .len(),
                2,
                "Gold Coins are created after the two earlier printed modes complete"
            );
        } else {
            let before = format!("{board:?}");
            board
                .answer_internal_card(
                    modes.actor,
                    modes.token,
                    Answer::Modes(vec![ModeId(1), ModeId(2), ModeId(3)]),
                )
                .unwrap_err();
            assert_eq!(
                format!("{board:?}"),
                before,
                "three modes cannot bypass the printed maximum"
            );
        }
    }
}
