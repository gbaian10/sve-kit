#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
//! Cards needing only the result-card, mode-condition and opponent-discard wiring.
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn seat(n: u32) -> PlayerId {
    PlayerId { seat: n }
}

fn cards(prefix: &str, numbers: &[&str]) -> Value {
    Value::Array(
        numbers
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("{prefix}{i}"),"card":card}))
            .collect(),
    )
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-batch-01",
    )
    .unwrap()
}

/// Answers every decision with the first legal members, or the given modes.
fn finish(game: &mut Game, mut step: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().map(|m| ModeId(*m)).collect()),
            InputDomain::Targets(domain) => {
                let most = domain
                    .cardinality
                    .maximum
                    .map_or(domain.members.len(), |m| usize::try_from(m.0).unwrap());
                let least = usize::try_from(domain.cardinality.minimum.0).unwrap();
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(most.max(least).min(domain.members.len()))
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(
                        usize::try_from(domain.cardinality.minimum.0)
                            .unwrap()
                            .max(1),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(1)
                    .map(|card| card.handle)
                    .collect(),
            )),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, modes);
    }
}

fn play(initial: &Value, modes: &[u64]) -> Game {
    let mut game = game(initial);
    let step = game.play_internal_card("source", seat(1)).unwrap();
    finish(&mut game, step, modes);
    triggers(&mut game, modes);
    game
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = cards("d", &["BP01-171", "BP01-171", "BP01-171"]);
    initial
}

#[test]
fn cp01_056_fanfare_weakens_an_opposing_follower() {
    let mut initial = in_hand("CP01-056");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-126"}]);
    let state = play(&initial, &[]).projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 3_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
}
