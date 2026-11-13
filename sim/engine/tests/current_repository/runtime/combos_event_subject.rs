#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-event-subject",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial
}

/// Answers every question, picking target candidates by printed card number.
fn drive(game: &mut Game, mut step: Option<CardPlay>, picks: &[&str]) {
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .filter(|m| m.card.as_deref().is_some_and(|c| picks.contains(&c)))
                        .take(1)
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
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

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    drive(game, Some(step), &[]);
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn has_keyword(view: &Value, id: &str, keyword: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .is_some_and(|keywords| keywords.contains(&json!(keyword)))
}

// One real-card scenario per printed card; the non-matching subjects live in the synthetic
// `internal_ir` event subject condition cases.
#[test]
fn bp09_002_grows_more_when_the_entering_follower_is_a_beast() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-002"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"entering","card":"BP04-127"}]);
    let mut game = loaded(&initial);
    play(&mut game, "entering");
    let view = state(&game);
    assert_eq!(view["objects"]["source"]["power"], 4_u8);
    assert_eq!(view["objects"]["source"]["hp"], 4_u8);
}

#[test]
fn bp10_077_buffs_and_grants_rush_to_an_entering_ghost() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-077"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"entering","card":"BP07-084"}]);
    let mut game = loaded(&initial);
    play(&mut game, "entering");
    let view = state(&game);
    assert_eq!(view["objects"]["entering"]["power"], 3_u8);
    assert!(has_keyword(&view, "entering", "突進"));
    assert_eq!(view["objects"]["source"]["power"], 5_u8);
}

#[test]
fn ecp02_050_buffs_an_entering_named_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP02-050"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"entering","card":"CP02-083"}]);
    let mut game = loaded(&initial);
    play(&mut game, "entering");
    let view = state(&game);
    assert_eq!(view["objects"]["entering"]["power"], 4_u8);
    assert_eq!(view["objects"]["entering"]["hp"], 3_u8);
}

#[test]
fn ecp02_051_evolution_look_entry_triggers_its_own_buff() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP02-050"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"ECP02-051"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"teru","card":"CP02-083"},
        {"id":"other1","card":"BP01-048"},
        {"id":"other2","card":"BP01-048"},
        {"id":"other3","card":"BP01-048"}
    ]);
    let mut game = loaded(&initial);
    let outcome = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(outcome.outcome, "resolved", "the real base evolves");
    drive(&mut game, None, &["CP02-083"]);
    let view = state(&game);
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("teru"))
    );
    assert_eq!(view["objects"]["teru"]["power"], 4_u8);
    assert_eq!(view["objects"]["teru"]["hp"], 3_u8);
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 3);
}
