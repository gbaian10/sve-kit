use super::*;
use sve_engine::game::player::{Input, QuestionPurpose};

#[test]
fn cp03_101_filters_its_private_drive_payment_before_drawing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-101"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"material","card":"CP03-101"},{"id":"other","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "drive-discard",
    )
    .unwrap();
    let ability = selection().current["CP03-101"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(ride) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ride payment")
    };
    resolved(
        game.answer_internal_card(
            ride.actor,
            ride.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Drive trigger")
    };
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("optional discard payment")
    };
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["source"]["power"], 3_u8);
    assert_eq!(before["P1"]["deck"], json!(["first", "second"]));
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let CardPlay::Awaiting(material) = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("hand materials")
    };
    assert!(game.internal_decision(View::P2).is_none());
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("payer's player question")
    };
    assert_eq!(question.purpose, QuestionPurpose::ResolutionChoice);
    let InputDomain::CostMaterials(candidates) = &material.domain else {
        panic!("private hand cost")
    };
    assert_eq!(candidates.members.len(), 1);
    assert_eq!(candidates.members[0].card.as_deref(), Some("CP03-101"));
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![candidates.members[0].handle]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["material"]));
    assert_eq!(after["P1"]["hand"], json!(["other", "first", "second"]));
}
