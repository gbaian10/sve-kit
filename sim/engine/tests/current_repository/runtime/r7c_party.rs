#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Party scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn initial(number: &str, spell_decoy: bool, full: bool) -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"grea-a","card":"BP13-042"}, {"id":"grea-b","card":"BP13-042"},
        {"id":"three","card":"BP21-100"}, {"id":"one","card":"BP21-115"},
        {"id":"decoy","card":if spell_decoy {"BP17-054"}else{"BP01-171"}},
        {"id":"tail","card":"BP21-115"}
    ]);
    if full {
        position["players"]["P1"]["zones"]["field"] = json!(
            (0..3_u8)
                .map(|n| json!({"id":format!("f{n}"),"card":"BP01-080"}))
                .collect::<Vec<_>>()
        );
    }
    position
}

fn looked_choice(board: &mut Game, question: &DecisionView, take: usize, full: bool) -> Answer {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private Look targets")
    };
    assert_eq!(
        domain.members.len(),
        4,
        "only the four Academy followers in the top five are eligible"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(if full { 2 } else { 3 })),
        "field capacity caps the printed maximum of three"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot see a private Look choice"
    );
    let grea = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP13-042"))
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    let three = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP21-100"))
        .unwrap()
        .handle;
    let one = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP21-115"))
        .unwrap()
        .handle;
    assert_eq!(
        grea.len(),
        2,
        "both two-cost copies are independently selectable"
    );
    let before = board.projection(View::Referee).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![grea[0], grea[1], three]),
        )
        .unwrap_err();
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "a total of seven, or three cards without space, consumes nothing"
    );
    assert_eq!(
        board.internal_decision(View::P1).unwrap().token,
        question.token,
        "an invalid sum preserves the current question"
    );
    Answer::Targets(match take {
        0 => vec![],
        1 => vec![three],
        2 => vec![grea[0], three],
        _ => vec![grea[0], three, one],
    })
}

fn finish_look(board: &mut Game, mut play: CardPlay, take: usize, full: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(_) => looked_choice(board, &question, take, full),
            InputDomain::Integer(_) => Answer::Integer(Nat(1)),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Party Look decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "the complete Party spell resolves before unrelated Fanfare choices"
    );
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

#[test]
fn party_all_printings_enforce_the_cost_sum_and_preserve_private_bottom_order() {
    for number in ["BP21-043", "PR-571"] {
        for (take, spell_decoy, full) in [
            (0_usize, true, false),
            (1, true, false),
            (3, true, false),
            (3, false, false),
            (2, true, true),
        ] {
            let mut board = game(&initial(number, spell_decoy, full));
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish_look(&mut board, play, take, full);
            let state = board.projection(View::Referee).unwrap();
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), take + if full { 3 } else { 0 });
            if take == 3 {
                assert!(
                    field.contains(&json!("three")) && field.contains(&json!("one")),
                    "three cards with original total six entered"
                );
                assert!(
                    field.contains(&json!("grea-a")) || field.contains(&json!("grea-b")),
                    "one two-cost copy completes the total of six"
                );
            }
            let mut expected = vec![json!("tail")];
            expected.extend(
                ["grea-a", "grea-b", "three", "one", "decoy"]
                    .into_iter()
                    .rev()
                    .filter(|id| !field.contains(&json!(id)))
                    .map(|id| json!(id)),
            );
            assert_eq!(state["P1"]["deck"], json!(expected));
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            let public = board.projection(View::P2).unwrap();
            for id in ["grea-a", "grea-b", "three", "one", "decoy", "tail"] {
                if !field.contains(&json!(id)) {
                    assert!(
                        public["objects"].get(id).is_none(),
                        "unselected Look identities remain private"
                    );
                }
            }
        }
    }
}

fn finish_discard(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Party discard decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn settle_discard(board: &mut Game) {
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed discard trigger")
        };
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_discard(board, play);
    }
}

#[test]
fn party_all_printings_draw_only_when_discarded_by_the_owners_academy_ability() {
    for number in ["BP21-043", "PR-571"] {
        for (source_card, academy) in [("BP13-042", true), ("CP01-036", false)] {
            let mut position = draw_position();
            position["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"source","card":source_card},{"id":"party","card":number}]);
            position["players"]["P1"]["zones"]["deck"] = json!([{"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]);
            position["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
            let mut board = game(&position);
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish_discard(&mut board, play);
            settle_discard(&mut board);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["cemetery"], json!(["party"]));
            assert_eq!(
                state["P1"]["hand"],
                if academy {
                    json!(["d0", "d1"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["deck"],
                if academy {
                    json!(["d2"])
                } else {
                    json!(["d0", "d1", "d2"])
                }
            );
        }
    }
}
