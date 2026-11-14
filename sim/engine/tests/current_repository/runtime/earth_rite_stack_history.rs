#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

/// BP14-037 stands with its evolved form ready; one Stack was already removed this turn.
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP14-037"},
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"golems","card":"BP01-063"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-038"}]);
    initial["semantic_state"]["counters_this_turn"] = json!({"P1":{"earth_rite_stack_removed":1_u8}});
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "earth-rite-stack-history",
    )
    .unwrap()
}

/// Plays BP01-063, answers its optional Earth Rite with `pay` and lets both golems enter standing.
fn cast(game: &mut Game, pay: bool) {
    let mut play = game
        .play_internal_card("golems", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(pay),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Quick { .. } => Answer::Quick(None),
            // Each Ward golem token may enter acted; the entry posture does not matter here.
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn evolve(game: &mut Game) -> String {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
    )
    .unwrap()
    .outcome
}

#[test]
fn bp14_037_evolves_only_after_two_stack_removed_by_own_earth_rite() {
    // Q1895-Q1897: the total over this turn's own Earth Rite payments counts, not one payment.
    for pay in [false, true] {
        let mut game = game(&position());
        assert_eq!(evolve(&mut game), "cannot-evolve", "one Stack is not enough");
        cast(&mut game, pay);
        if pay {
            assert_eq!(evolve(&mut game), "resolved", "a second Stack reaches two");
        } else {
            assert_eq!(evolve(&mut game), "cannot-evolve", "declined Earth Rite removes none");
        }
    }
}
