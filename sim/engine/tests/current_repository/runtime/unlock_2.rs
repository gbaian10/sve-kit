#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game_with_rules(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "unlock-2",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a printed decision");
    };
    question
}

fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a printed trigger");
    };
    assert_eq!(options.len(), 1, "one printed trigger is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp22_035_banishes_only_an_ex_soldier_follower_before_drawing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"soldier","card":"BP01-038"},
        {"id":"commander","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"field-soldier","card":"BP01-038"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    for (number, accepted) in [("BP22-035", false), ("BP22-035", true), ("BP22-P18", true)] {
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        let mut game = game_with_rules(&initial);
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(entry, CardPlay::Entered(_)));
        let declaration = awaiting(trigger(&mut game));
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let payment = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap();
        if !accepted {
            assert_eq!(payment, CardPlay::Cancelled);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"], json!(["soldier", "commander"]));
            assert_eq!(state["P1"]["hand"], json!([]));
            continue;
        }
        let materials = awaiting(payment);
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("expected material payment");
        };
        assert_eq!(domain.members.len(), 1);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["ex"], json!(["soldier", "commander"]));
        assert_eq!(before["P1"]["hand"], json!([]));
        game.answer_internal_card(
            PlayerId { seat: 2 },
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        resolved(
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["soldier"]));
        assert_eq!(state["P1"]["ex"], json!(["commander"]));
        assert_eq!(state["P1"]["field"], json!(["field-soldier", "source"]));
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn bp17_015_creates_a_tree_then_acts_and_banishes_a_field_tree_to_draw() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-015"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"tree","card":"BP07-T03"},
        {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},
        {"id":"loot","card":"BP01-037"}
    ]);
    let mut game = game_with_rules(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(trigger(&mut game));
    game.resume_internal_card().unwrap();
    let created = game.projection(View::Referee).unwrap();
    let token_id = created["P1"]["ex"][0].as_str().unwrap().to_owned();
    assert_eq!(created["objects"][&token_id]["name"], "ナテラの大樹");
    assert_eq!(created["P1"]["ex"].as_array().unwrap().len(), 1);
    let ability = selection_with_rules().current["BP17-015"]
        .program
        .card()
        .abilities
        .iter()
        .find(|candidate| candidate.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let materials = awaiting(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected the field tree payment");
    };
    assert_eq!(domain.members.len(), 1);
    let declared = game.projection(View::Referee).unwrap();
    assert_eq!(declared["P1"], created["P1"]);
    assert_eq!(declared["objects"], created["objects"]);
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P1"]["field"], json!(["other", "source"]));
    assert_eq!(state["P1"]["banish"], json!([]));
    assert_eq!(state["P1"]["ex"], json!([token_id]));
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    game.resume_internal_card().unwrap();
    let discard = awaiting(trigger(&mut game));
    let InputDomain::Targets(choices) = &discard.domain else {
        panic!("expected the departed tree's discard");
    };
    assert_eq!(choices.members.len(), 2);
    resolved(
        game.answer_internal_card(
            discard.actor,
            discard.token,
            Answer::Targets(vec![choices.members[0].handle]),
        )
        .unwrap(),
    );
    let final_state = game.projection(View::Referee).unwrap();
    assert_eq!(final_state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(final_state["P1"]["cemetery"].as_array().unwrap().len(), 1);
    assert_eq!(final_state["P1"]["deck"], json!([]));
}
