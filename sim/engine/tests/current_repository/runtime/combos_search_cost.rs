#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-search-cost",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial
}

/// Plays `source`, picking the first target and recording every searchable card number.
fn play(game: &mut Game) -> Vec<String> {
    let mut searchable = Vec::new();
    let mut step = Some(game.play_internal_card("source", PLAYER).unwrap());
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    Answer::Targets(d.members.iter().take(1).map(|m| m.handle).collect())
                }
                InputDomain::Search(d) => {
                    searchable.extend(d.cards.iter().map(|card| card.card.clone()));
                    Answer::Search(SearchAnswer::Ungrouped(
                        d.cards.iter().take(1).map(|card| card.handle).collect(),
                    ))
                }
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Quick { .. } => Answer::Quick(None),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return searchable;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp12_114_searches_a_follower_costing_the_chosen_enemy_original_cost() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-114"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"five","card":"BP01-037"},
        {"id":"three","card":"BP01-048"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-048"}]);
    let mut game = loaded(&initial);
    let searchable = play(&mut game);
    assert_eq!(searchable, ["BP01-048"]);
    let view = state(&game);
    assert_eq!(view["P1"]["ex"], json!(["three"]));
    assert_eq!(view["P1"]["deck"], json!(["five"]));
}

#[test]
fn bp13_053_destroys_a_chess_follower_and_fields_one_costing_two_more() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-053"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"bishop","card":"BP03-052"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"knight","card":"BP03-046"},
        {"id":"queen","card":"BP03-042"},
        {"id":"other","card":"BP01-037"}
    ]);
    let mut game = loaded(&initial);
    let searchable = play(&mut game);
    assert_eq!(searchable, ["BP03-042"]);
    let view = state(&game);
    // The queen's own fanfare adds a pawn token beside it.
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("queen"))
    );
    assert!(!view["P1"]["field"].as_array().unwrap().contains(&json!("bishop")));
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("bishop"))
    );
}
