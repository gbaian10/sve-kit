#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

/// Erin stands on P1's field; Tia and a second Erin wait in hand with their evolved forms ready.
fn position(erin: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":erin}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"tia","card":"BP02-001"},
        {"id":"twin","card":"BP22-003"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"tia-form","card":"BP02-002"},
        {"id":"erin-form","card":"BP22-004"}
    ]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "entry-subject-evolution",
    )
    .unwrap()
}

/// Takes the first evolved form offered when `evolve`, otherwise declines (Q2789).
fn settle(game: &mut Game, mut step: Option<CardPlay>, evolve: bool) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .filter(|_| evolve)
                        .map(|m| m.handle)
                        .take(1)
                        .collect(),
                ),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
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
            return;
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

fn play(game: &mut Game, id: &str, evolve: bool) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    settle(game, Some(step), evolve);
}

fn evolve_deck(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["P1"]["evolve_deck"].clone()
}

#[test]
fn bp22_003_evolves_another_entering_crystalia_follower() {
    let mut game = game(&position("BP22-003"));
    play(&mut game, "tia", true);
    assert_eq!(evolve_deck(&game), json!(["erin-form"]), "Tia used her form");
}

#[test]
fn bp22_003_may_leave_the_entering_follower_unevolved() {
    // Q2789: evolving through the effect can be declined.
    let mut game = game(&position("BP22-003"));
    play(&mut game, "tia", false);
    assert_eq!(evolve_deck(&game), json!(["tia-form", "erin-form"]));
}

#[test]
fn bp22_003_ignores_an_entering_erin() {
    // 「これと同名を除く」: a second Erin does not trigger the first even when a form is offered.
    let mut game = game(&position("BP22-003"));
    play(&mut game, "twin", true);
    assert_eq!(evolve_deck(&game), json!(["tia-form", "erin-form"]));
}

#[test]
fn bp22_004_discounts_the_next_crystalia_card_and_evolves_it() {
    let mut initial = position("BP22-003");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"tia-form","card":"BP02-002"},
        {"id":"form","card":"BP22-004"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    settle(&mut game, None, true);
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    play(&mut game, "tia", true);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"].as_u64().unwrap(), before, "2 - 3 costs nothing");
    assert_eq!(view["P1"]["evolve_deck"], json!([]), "the evolved Erin still evolves Tia");
}
