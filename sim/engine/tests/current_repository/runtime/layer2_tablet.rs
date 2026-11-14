#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real Tablet allocation scenarios."
)]
use super::*;
use sve_engine::game::internal::{InstructionOutcome, SearchAnswer};

fn position(field: u8, ex: u8) -> Value {
    let mut initial = draw_position();
    let mut own = vec![json!({"id":"tablet","card":"BP20-117"})];
    own.extend((0_u8..field).map(|i| json!({"id":format!("field{i}"),"card":"BP01-037"})));
    initial["players"]["P1"]["zones"]["field"] = json!(own);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0_u8..ex)
            .map(|i| json!({"id":format!("ex{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"ezdia","card":"BP20-001"},
        {"id":"lishenna","card":"BP20-037"},
        {"id":"marwynn","card":"BP20-093"},
        {"id":"duplicate","card":"BP20-001"},
        {"id":"wrong","card":"BP01-037"}
    ]);
    initial
}

fn allocate(game: &mut Game, found: usize, field: usize, ex: usize, free: [u64; 2]) -> Activation {
    let ability = game.internal_activated_sources("tablet").unwrap()[0].definition;
    let mut play = game
        .activate_internal_card("tablet", ability, PlayerId { seat: 1 })
        .unwrap();
    let mut partition = 0_usize;
    loop {
        let CardPlay::Awaiting(question) = play else {
            return resolved(play);
        };
        let answer = match &question.domain {
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    4,
                    "only the named Omen followers qualify"
                );
                let duplicate = domain
                    .cards
                    .iter()
                    .filter(|m| m.card == "BP20-001")
                    .map(|m| m.handle)
                    .collect::<Vec<_>>();
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Search(SearchAnswer::Grouped(vec![duplicate])),
                )
                .unwrap_err();
                let handles = ["BP20-001", "BP20-037", "BP20-093"]
                    .into_iter()
                    .take(found)
                    .map(|number| {
                        domain
                            .cards
                            .iter()
                            .find(|m| m.card == number)
                            .unwrap()
                            .handle
                    })
                    .collect();
                Answer::Search(SearchAnswer::Grouped(vec![handles]))
            }
            InputDomain::SearchPartition(domain) => {
                assert_eq!(
                    domain.cardinality.minimum,
                    Nat(0),
                    "each optional destination permits leaving all found cards in the deck"
                );
                assert_eq!(
                    domain.cardinality.maximum,
                    Some(Nat(free[partition]
                        .min(u64::try_from(domain.cards.len()).unwrap())
                        .min(10))),
                    "the declared upper bound is capped by remaining cards and destination capacity"
                );
                let count = if partition == 0 { field } else { ex };
                if domain.cards.len()
                    > usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap()
                {
                    let too_many = domain.cards.iter().map(|m| m.handle).collect();
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::SearchPartition(too_many),
                    )
                    .unwrap_err();
                }
                partition = partition.checked_add(1).unwrap();
                Answer::SearchPartition(domain.cards.iter().take(count).map(|m| m.handle).collect())
            }
            _ => panic!("unexpected Tablet decision: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

#[test]
fn bp20_117_allocates_one_found_set_in_printed_order_and_leaves_unassigned_cards() {
    let mut game = game(&position(0, 0));
    let result = allocate(&mut game, 3, 1, 1, [5, 5]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["tablet"]));
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 3);
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|f| matches!(f, Fact::Shuffled { .. }))
            .count(),
        1
    );
    let InstructionOutcome::Search(outcome) = result
        .progress
        .receipts
        .values()
        .find(|r| matches!(r, InstructionOutcome::Search(_)))
        .unwrap()
    else {
        panic!("search receipt");
    };
    assert_eq!(outcome.found.len(), 3);
    assert_eq!(outcome.moved.len(), 2);
}

#[test]
fn bp20_117_limits_each_optional_partition_to_current_capacity() {
    let mut game = game(&position(4, 4));
    allocate(&mut game, 3, 1, 1, [1, 1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 3);
}

#[test]
fn bp20_117_may_leave_every_found_card_in_deck_even_when_destinations_have_space() {
    let mut game = game(&position(0, 0));
    allocate(&mut game, 3, 0, 0, [5, 5]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
}

#[test]
fn bp20_117_finding_zero_still_pays_cost_and_shuffles_once() {
    let mut game = game(&position(0, 0));
    let result = allocate(&mut game, 0, 0, 0, [5, 5]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["tablet"]));
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|f| matches!(f, Fact::Shuffled { .. }))
            .count(),
        1
    );
}

#[test]
fn bp20_117_full_ex_accepts_zero_and_keeps_the_remaining_found_cards() {
    let mut game = game(&position(4, 5));
    allocate(&mut game, 3, 1, 0, [1, 0]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 4);
}

#[test]
fn bp20_117_all_field_choices_finish_before_their_fanfares_can_be_played() {
    let mut game = game(&position(0, 0));
    allocate(&mut game, 3, 2, 0, [5, 5]);
    assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("both entered fanfares");
    };
    assert_eq!(options.len(), 2);
    assert!(options.iter().any(|option| option.card == "BP20-001"));
    let lishenna = options
        .iter()
        .find(|option| option.card == "BP20-037")
        .unwrap();
    let CardPlay::Awaiting(modes) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(lishenna.handle),
        )
        .unwrap()
    else {
        panic!("Lishenna may be played before Ezdia");
    };
    assert!(matches!(modes.domain, InputDomain::Modes { .. }));
}
