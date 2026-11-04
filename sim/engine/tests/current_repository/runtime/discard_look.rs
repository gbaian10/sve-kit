#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the complete printed discard-Look scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-discard-look",
    )
    .unwrap()
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"ritual","card":"BP02-062"},{"id":"source","card":number}
    ]);
    initial
}

fn pending(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let selected = options
        .iter()
        .find(|entry| entry.card == number && entry.line == line)
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(selected.handle))
        .unwrap()
}

fn discard_with_ritual(board: &mut Game, number: &str) {
    assert!(
        matches!(
            board
                .play_internal_card("ritual", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the complete real Ritual must enter before its Fanfare is selected"
    );
    let mut play = pending(board, "BP02-062", 2);
    while let CardPlay::Awaiting(q) = play {
        let answer = if matches!(q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            let InputDomain::CostMaterials(domain) = &q.domain else {
                panic!("real Ritual discard cost");
            };
            let material = domain
                .members
                .iter()
                .find(|entry| entry.card.as_deref() == Some(number))
                .unwrap();
            Answer::CostMaterials(vec![material.handle])
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    resolved(play);
    finish_checks(board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the real cost must discard the requested card before its trigger resolves"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 21_u8,
        "the complete real helper finishes its heal before the discard trigger"
    );
}

fn finish_look(board: &mut Game, mut play: CardPlay, expected: Option<&str>, accept: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let cards = domain
                .members
                .iter()
                .map(|entry| entry.card.as_deref().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                cards,
                expected.into_iter().collect::<Vec<_>>(),
                "the real Look exposes only the printed eligible cards"
            );
            assert!(
                board.internal_decision(View::P2).is_none(),
                "the opponent cannot inspect the private candidates"
            );
            Answer::Targets(if accept {
                domain
                    .members
                    .first()
                    .map(|entry| vec![entry.handle])
                    .unwrap_or_default()
            } else {
                vec![]
            })
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|entry| entry.handle).collect())
        } else if matches!(q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected printed Look choice: {:?}", q.domain);
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    resolved(play);
    finish_checks(board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "every printed occurrence has finished without a leftover choice"
    );
}

#[test]
fn zahar_discard_uses_original_cost_and_dragon_class_then_orders_the_actual_remainder() {
    for other in ["BP12-052", "BP01-051"] {
        for accept in [false, true] {
            let mut initial = position("SD08-011");
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"high","card":"BP01-080","state":{"cost":0_u8}},
                {"id":"other","card":other,"state":{"cost":99_u8}},
                {"id":"tail","card":"BP01-171"}
            ]);
            let mut board = game(&initial);
            discard_with_ritual(&mut board, "SD08-011");
            let trigger = pending(&mut board, "SD08-011", 1);
            finish_look(&mut board, trigger, Some("BP01-080"), accept);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["hand"],
                if accept { json!(["high"]) } else { json!([]) }
            );
            assert_eq!(
                view["P1"]["deck"],
                if accept {
                    json!(["tail", "other"])
                } else {
                    json!(["tail", "other", "high"])
                }
            );
        }
    }
}

#[test]
fn zahar_fanfare_excludes_its_name_and_spells_and_puts_the_other_follower_without_paying_its_cost()
{
    for accept in [false, true] {
        let mut initial = position("SD08-011");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"SD08-011"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"same","card":"SD08-011"}, {"id":"high","card":"BP01-080"},
            {"id":"spell","card":"ECP01-036"}, {"id":"tail","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let trigger = pending(&mut board, "SD08-011", 2);
        finish_look(&mut board, trigger, Some("BP01-080"), accept);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 2_u8,
            "only Zahar's printed PP8 is paid"
        );
        assert_eq!(
            view["P1"]["field"],
            if accept {
                json!(["source", "high"])
            } else {
                json!(["source"])
            }
        );
        assert_eq!(
            view["P1"]["deck"],
            if accept {
                json!(["tail", "spell", "same"])
            } else {
                json!(["tail", "spell", "high", "same"])
            }
        );
    }
}

#[test]
fn geno_discard_may_take_a_levin_card_and_otherwise_leaves_the_single_card_on_top() {
    for revealed in ["BP13-033", "BP01-171"] {
        for accept in [false, true] {
            let mut initial = position("SD07-012");
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"seen","card":revealed},{"id":"tail","card":"BP01-171"}
            ]);
            let mut board = game(&initial);
            discard_with_ritual(&mut board, "SD07-012");
            let trigger = pending(&mut board, "SD07-012", 1);
            finish_look(
                &mut board,
                trigger,
                (revealed == "BP13-033").then_some(revealed),
                accept,
            );
            let taken = accept && revealed == "BP13-033";
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["hand"],
                if taken { json!(["seen"]) } else { json!([]) }
            );
            assert_eq!(
                view["P1"]["deck"],
                if taken {
                    json!(["tail"])
                } else {
                    json!(["seen", "tail"])
                },
                "the printing has no instruction to put an unchosen card on the bottom"
            );
        }
    }
}

#[test]
fn geno_counts_only_own_cemetery_levin_cards_including_spells_for_storm() {
    for count in [0_u8, 4, 5] {
        let mut initial = position("SD07-012");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"SD07-012"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((0_u8..count).map(|i|
            json!({"id":format!("own{i}"),"card":if i==4 {"BP16-026"} else if i<2 {"BP13-033"} else {"BP16-021"}})
        ).collect::<Vec<_>>());
        if count == 0 {
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"wrong0","card":"BP01-171"},{"id":"wrong1","card":"BP01-171"},
                {"id":"wrong2","card":"BP01-171"},{"id":"wrong3","card":"BP02-062"},
                {"id":"wrong4","card":"BP02-062"}
            ]);
        }
        initial["players"]["P2"]["zones"]["cemetery"] = json!([
            {"id":"foreign0","card":"BP13-033"},{"id":"foreign1","card":"BP13-033"},
            {"id":"foreign2","card":"BP16-021"},{"id":"foreign3","card":"BP16-021"},
            {"id":"foreign4","card":"BP16-026"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "normal play is not a discard"
        );
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            count == 5
        );
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        if count == 5 {
            assert_eq!(
                board
                    .internal_rule_action(
                        &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                    )
                    .unwrap()
                    .outcome,
                "resolved"
            );
        }
    }
}
