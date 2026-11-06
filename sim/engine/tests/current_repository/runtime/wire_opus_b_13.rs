#![expect(
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn position(printing: &str, life: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(life);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"kurumi","card":"CP04-107"},
        {"id":"suzume","card":"CP04-105"},
        {"id":"ayane","card":"CP04-070"}
    ]);
    initial
}

/// Plays the source to its fanfare and returns the printed mode question.
fn fanfare(game: &mut Game) -> CardPlay {
    let played = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(played, CardPlay::Entered(_)), "{played:?}");
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("fanfare");
    };
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

/// Answers the mode question with `modes` (by position) and the search with `found` cards.
fn answer(game: &mut Game, mut step: CardPlay, modes: &[usize], found: &[&str]) -> Result<(), EngineFailure> {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Modes { modes: offered, .. } => {
                Answer::Modes(modes.iter().map(|&mode| offered[mode]).collect())
            }
            InputDomain::Search(d) => {
                let picked: Vec<u64> = found
                    .iter()
                    .map(|card| d.cards.iter().find(|c| c.card == *card).unwrap().handle)
                    .collect();
                // The original-cost total is one printed group over every found card.
                Answer::Search(d.groups.as_ref().map_or_else(
                    || SearchAnswer::Ungrouped(picked.clone()),
                    |groups| {
                        SearchAnswer::Grouped(
                            groups
                                .iter()
                                .map(|g| {
                                    picked
                                        .iter()
                                        .copied()
                                        .filter(|h| g.handles.contains(h))
                                        .collect()
                                })
                                .collect(),
                        )
                    },
                ))
            }
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer)?;
    }
    finish_checks(game);
    Ok(())
}

fn offered_modes(step: &CardPlay) -> u64 {
    let CardPlay::Awaiting(q) = step else {
        panic!("mode question");
    };
    let InputDomain::Modes { cardinality, .. } = &q.domain else {
        panic!("mode question: {:?}", q.domain);
    };
    cardinality.maximum.unwrap().0
}

fn field(view: &Value) -> Vec<String> {
    let mut ids: Vec<String> = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

fn equipped(view: &Value) -> bool {
    view["P1"]["equipment"]
        .as_array()
        .unwrap()
        .iter()
        .any(|token| view["objects"][token.as_str().unwrap()]["equipped_to"] == "source")
}

#[test]
fn cp04_091_all_printings_choose_one_mode_above_ten_life() {
    for printing in ["CP04-091", "CP04-SL21", "PCS01-039"] {
        // Equip only.
        let mut equip = game(&position(printing, 11));
        let modes = fanfare(&mut equip);
        assert_eq!(offered_modes(&modes), 1, "{printing}");
        answer(&mut equip, modes, &[0], &[]).unwrap();
        let after_equip = equip.projection(View::Referee).unwrap();
        assert!(equipped(&after_equip), "{printing}");
        assert_eq!(after_equip["P1"]["field"], json!(["source"]), "{printing}");
        // Search only: original costs 1 + 2 fit the total of 3.
        let mut search = game(&position(printing, 11));
        let step = fanfare(&mut search);
        answer(&mut search, step, &[1], &["CP04-107", "CP04-105"]).unwrap();
        let view = search.projection(View::Referee).unwrap();
        assert!(!equipped(&view), "{printing}");
        assert_eq!(field(&view), ["kurumi", "source", "suzume"], "{printing}");
    }
}

#[test]
fn cp04_091_takes_both_modes_at_ten_life() {
    let mut game = game(&position("CP04-091", 10));
    let step = fanfare(&mut game);
    assert_eq!(offered_modes(&step), 2);
    answer(&mut game, step, &[0, 1], &["CP04-107", "CP04-070"]).unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert!(equipped(&view));
    assert_eq!(field(&view), ["ayane", "kurumi", "source"]);
}

#[test]
fn cp04_091_rejects_two_followers_over_the_original_cost_total() {
    // 2 + 2 exceeds the total of 3.
    let mut game = game(&position("CP04-091", 11));
    let step = fanfare(&mut game);
    answer(&mut game, step, &[1], &["CP04-105", "CP04-070"]).unwrap_err();
}
