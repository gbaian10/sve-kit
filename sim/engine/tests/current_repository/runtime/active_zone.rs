#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
//! Abilities active outside the hand-and-field defaults: EX triggers and Advance activations.
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "active-zone",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain.members.iter().take(1).map(|m| m.handle).collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
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
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
    }
}

fn enemies(initial: &mut Value) {
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-126"},{"id":"small","card":"BP01-171"}
    ]);
}

#[test]
fn bp10_110_end_phase_in_ex_damages_every_opponent_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"world","card":"BP10-110"}]);
    enemies(&mut initial);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["small"]), "the 2/2 is destroyed");
}

#[test]
fn bp10_059_advance_follower_activates_on_the_field() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP10-059"},{"id":"weapon","card":"BP02-T07"}
    ]);
    enemies(&mut initial);
    let mut game = game(&initial);
    let ability = selection().current["BP10-059"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", ability, actor()).unwrap();
    finish(&mut game, step);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]), "the Dragoweapon token pays the cost");
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["objects"]["source"]["power"], 8_u8);
    assert_eq!(state["objects"]["source"]["hp"], 8_u8);
}

#[test]
fn bp10_059_has_storm_with_five_armed_cards_in_its_cemetery() {
    for (count, storm) in [(4_usize, false), (5, true)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-059"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..count)
                .map(|i| json!({"id":format!("arm{i}"),"card":"BP03-066"}))
                .collect(),
        );
        let state = game(&initial).projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            storm
        );
    }
}

#[test]
fn cp04_007_triggers_when_it_is_placed_into_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-084"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"anemone","card":"CP04-007"},{"id":"rest","card":"BP01-171"}]);
    let mut game = game(&initial);
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, step);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["anemone"]));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8, "its EX ability damages the opponent");
    assert_eq!(state["P1"]["leader"]["life"], 21_u8, "and heals its own leader");
}

#[test]
fn ex_zone_movement_abilities_observe_only_their_own_arrival() {
    let source = inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "CP04-007")
        .unwrap();
    let mut candidate = source["input"].clone();
    lowerer().lower_current(&candidate).unwrap();
    let face = candidate["tree"]["cards"]["CP04-007"]["faces"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let auto = face["abilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ability| ability.get("auto").is_some())
        .unwrap();
    // Watching any card enter EX would need a different observer set.
    auto["auto"]["on"]["placed_ex"] = json!({"who":"self"});
    assert_eq!(
        lowerer().lower_current(&candidate).unwrap_err().code,
        "unsupported_construct"
    );
}
