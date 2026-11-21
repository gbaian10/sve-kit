#![expect(
    clippy::panic,
    reason = "The fixture must reach the main action window."
)]

use super::*;
use sve_engine::game::player::{Input, MainActions, MainIntentKind, QuestionShape};

fn actions(initial: &Value) -> MainActions {
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "remaining-main-preflight",
    )
    .unwrap();
    game.advance_player_input();
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("main action question");
    };
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("main action window");
    };
    actions
}

fn offers_spell(actions: &MainActions, number: &str) -> bool {
    actions.intents.iter().any(|intent| {
        matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == number)
    })
}

fn offers_activation(initial: &Value, number: &str, printed_line: u64) -> bool {
    actions(initial).intents.iter().any(|intent| {
        matches!(
            &intent.kind,
            MainIntentKind::Activate { holder, line, .. }
                if holder.properties.card == number && *line == printed_line
        )
    })
}

#[test]
fn bp09_dragon_spells_are_offered_only_when_a_formal_target_exists() {
    let spells = ["BP09-058", "BP09-068"];
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"quick","card":spells[0]}, {"id":"claw","card":spells[1]}
    ]);
    let without_target = actions(&initial);
    assert_eq!(
        spells.map(|number| offers_spell(&without_target, number)),
        [false, false]
    );

    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let with_target = actions(&initial);
    assert_eq!(
        spells.map(|number| offers_spell(&with_target, number)),
        [true, true]
    );
}

#[test]
fn bp20_030_field_activation_requires_a_fusion_counter() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-030"}]);
    assert!(!offers_activation(&initial, "BP20-030", 1));

    initial["players"]["P1"]["zones"]["field"][0]["state"]["counters"] =
        json!({"融合カウンター":1_u8});
    assert!(offers_activation(&initial, "BP20-030", 1));
}

#[test]
fn cp04_036_requires_a_payable_exchange_or_enough_original_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP04-036"}]);
    assert!(!offers_spell(&actions(&initial), "CP04-036"));

    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    assert!(!offers_spell(&actions(&initial), "CP04-036"));

    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"fee","card":"CP04-019","state":{"acted":true}}]);
    assert!(!offers_spell(&actions(&initial), "CP04-036"));

    initial["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    assert!(offers_spell(&actions(&initial), "CP04-036"));

    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    assert!(offers_spell(&actions(&initial), "CP04-036"));
}
