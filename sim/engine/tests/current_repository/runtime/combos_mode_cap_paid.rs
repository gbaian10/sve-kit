#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(pp: u8) -> Game {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP21-026"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"emilia","card":"BP16-019"},
        {"id":"filler","card":"BP01-037"},
        {"id":"lisha","card":"BP12-020"}
    ]);
    Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "combos-mode-cap-paid",
    )
    .unwrap()
}

/// Plays the spell, answering the +2 declaration and the modes; returns the offered mode count.
fn play(game: &mut Game, boost: bool, modes: &[u64]) -> Option<u64> {
    let mut offered = None;
    let mut step = game.play_internal_card("source", PLAYER).unwrap();
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(boost),
            InputDomain::Modes { count, .. } => {
                offered = Some(count.0);
                Answer::Modes(modes.iter().copied().map(ModeId).collect())
            }
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards.iter().take(1).map(|c| c.handle).collect(),
            )),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected decision: {:?}", q.domain),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    offered
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp21_026_without_the_extra_cost_chooses_one_mode_for_four_pp() {
    let mut game = loaded(10);
    assert_eq!(play(&mut game, false, &[1]), Some(1));
    let view = state(&game);
    assert_eq!(view["P1"]["field"], json!(["emilia"]));
    assert_eq!(view["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp21_026_with_the_extra_cost_chooses_up_to_two_modes_for_six_pp() {
    let mut game = loaded(10);
    assert_eq!(play(&mut game, true, &[1, 2]), Some(2));
    let view = state(&game);
    let mut field: Vec<_> = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    field.sort();
    assert_eq!(field, ["emilia", "lisha"]);
    assert_eq!(view["P1"]["pp"]["current"], 4_u8);

    let mut single = loaded(10);
    assert_eq!(play(&mut single, true, &[2]), Some(2), "Up to two allows one");
    let after = state(&single);
    assert_eq!(after["P1"]["field"], json!(["lisha"]));
    assert_eq!(after["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn bp21_026_cannot_declare_the_extra_cost_without_six_pp() {
    let mut game = loaded(5);
    let refused = match game.play_internal_card("source", PLAYER) {
        Err(_) => true,
        Ok(CardPlay::Awaiting(q)) if matches!(q.domain, InputDomain::OptionalCost) => {
            match game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(true)) {
                Err(_) => true,
                Ok(CardPlay::Awaiting(q)) => {
                    // Any later question must still end in an unaffordable payment.
                    let answer = match &q.domain {
                        InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1), ModeId(2)]),
                        _ => panic!("unexpected decision: {:?}", q.domain),
                    };
                    game.answer_internal_card(q.actor, q.token, answer).is_err()
                }
                Ok(_) => false,
            }
        }
        Ok(_) => false,
    };
    assert!(refused, "Five PP cannot pay the boosted cost of six");
    let view = state(&game);
    assert_eq!(view["P1"]["field"], json!([]));
    assert_eq!(view["P1"]["pp"]["current"], 5_u8);
}
