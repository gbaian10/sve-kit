#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed searched-entry scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, decline: bool) -> usize {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Search(domain) => {
                assert!(
                    board.internal_decision(View::P2).is_none(),
                    "search remains private"
                );
                Answer::Search(SearchAnswer::Ungrouped(if decline {
                    vec![]
                } else {
                    domain
                        .cards
                        .first()
                        .map_or_else(Vec::new, |card| vec![card.handle])
                }))
            }
            InputDomain::Targets(domain) => {
                assert!(
                    board.internal_decision(View::P2).is_none(),
                    "the hand return remains private"
                );
                Answer::Targets(
                    domain
                        .members
                        .first()
                        .map_or_else(Vec::new, |card| vec![card.handle]),
                )
            }
            other => panic!("unexpected searched-entry decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    let shuffles = if let CardPlay::Resolved(result) = play {
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
            .count()
    } else {
        0
    };
    checks(board);
    shuffles
}

fn settle(board: &mut Game, decline: bool) -> usize {
    let mut shuffles = 0_usize;
    loop {
        checks(board);
        if let Some(question) = board.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("printed trigger ordering")
            };
            let play = board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            shuffles = shuffles.checked_add(finish(board, play, decline)).unwrap();
            continue;
        }
        let Some(question) = board.internal_quick_choice().unwrap() else {
            return shuffles;
        };
        let play = board
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        shuffles = shuffles.checked_add(finish(board, play, decline)).unwrap();
    }
}

fn tutankhamun(number: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number,"state":{"hp":1_u8,"max_hp":1_u8}}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"blaze","card":"BP01-089"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"copy","card":number},{"id":"drawn","card":"BP01-171"},{"id":"spare","card":"BP01-171"}]);
    game(&initial)
}

#[test]
fn bp09_088_all_prints_search_acted_and_resolve_the_entered_cards_full_fanfare() {
    for number in ["BP09-088", "BP09-SL18"] {
        let mut board = tutankhamun(number);
        let play = board
            .play_internal_card("blaze", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, false);
        assert_eq!(settle(&mut board, false), 1);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["copy"]));
        assert_eq!(state["objects"]["copy"]["acted"], true);
        assert_eq!(state["objects"]["copy"]["generation"], 1_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
        let remaining = state["P1"]["deck"].as_array().unwrap();
        assert_eq!(remaining.len(), 2);
        let returned = remaining.last().unwrap().as_str().unwrap();
        assert!(matches!(returned, "drawn" | "spare"));
        assert_eq!(state["objects"][returned]["generation"], 2_u8);
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    }
}

#[test]
fn bp09_088_all_prints_may_find_nothing_after_paying_and_still_shuffle() {
    for number in ["BP09-088", "BP09-SL18"] {
        let mut board = tutankhamun(number);
        let play = board
            .play_internal_card("blaze", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, true);
        assert_eq!(settle(&mut board, true), 1);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!([]));
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 3);
        assert_eq!(state["objects"]["copy"]["generation"], 0_u8);
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    }
}

#[test]
fn bp06_082_searches_the_real_base_acted_and_shuffles_without_a_match_q844() {
    for matching in [true, false] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-081"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-082"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"copy","card":if matching {"BP06-081"} else {"BP01-171"}}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(settle(&mut board, false), 1);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("copy")),
            matching
        );
        assert_eq!(state["objects"]["copy"]["acted"], matching);
        assert_eq!(state["objects"]["copy"]["generation"], u8::from(matching));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}
