use super::*;
use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind};

#[test]
fn phantom_blaster_dragon_evolves_after_paying_with_an_evolved_blaster_dark() {
    for replaced in [false, true] {
        let mut initial = position_with_source("CP03-084", "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"fee","card":"CP03-086","state":{"evolved":true,"evolved_with":"fee-form"}}
        ]);
        initial["players"]["P1"]["zones"]["evolution"] =
            json!([{"id":"fee-form","card":"CP03-SL19"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP03-085"}]);
        if replaced {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"fafnir","card":"BP18-061"}]);
        }
        let mut game = game(&initial);
        let play = game.play_internal_card("source", actor()).unwrap();
        let play = choose_card_by_number(&mut game, play, "CP03-086");
        assert!(matches!(accept(&mut game, play), CardPlay::Entered(_)));
        checks(&mut game);
        let fanfare = resolve_pending_card(&mut game, "CP03-084");
        let fanfare = choose_card_by_number(&mut game, fanfare, "CP03-085");
        drop(accept(&mut game, fanfare));
        checks(&mut game);

        let after = game.projection(View::Referee).unwrap();
        let destination = if replaced { "banish" } else { "cemetery" };
        assert_eq!(after["P1"][destination], json!(["fee"]));
        assert_eq!(after["P1"]["evolve_deck"], json!(["fee-form"]));
        assert_eq!(after["objects"]["fee-form"]["face_up"], true);
        assert_eq!(after["objects"]["source"]["evolved_with"], "form");
        assert_eq!(after["P1"]["pp"]["current"], 7_i64);
    }
}

#[test]
fn phantom_blaster_dragon_can_pay_with_a_follower_who_has_ridden() {
    let mut initial = position_with_source("CP03-084", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"rider","card":"CP03-P58"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("top", "BP01-037", 4));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"fafnir","card":"BP18-061"}]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ride-payment",
    )
    .unwrap();
    let ride = game
        .activate_internal_card("rider", activation("CP03-P58"), actor())
        .unwrap();
    let ride = await_card_choice(ride);
    let played = game
        .answer_internal_card(
            ride.actor,
            ride.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap();
    resolved(played);
    checks(&mut game);
    let drive_gained = resolve_pending_card(&mut game, "CP03-P58");
    if let CardPlay::Awaiting(choice) = drive_gained {
        resolved(
            game.answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![]))
                .unwrap(),
        );
    } else {
        resolved(drive_gained);
    }
    checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["drive"],
        json!(["point"])
    );

    game.advance_player_input();
    player_presentation::declare(&mut game, "CP03-084");
    let payment = player_presentation::question(&game);
    let QuestionShape::Decision(DecisionShape::CostMaterials(domain)) = payment.shape else {
        panic!("the card asks for its printed material");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(
        player_presentation::answer(
            &mut game,
            Answer::CostMaterials(vec![domain.members[0].handle])
        ),
        SubmissionKind::Applied
    );

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P1"]["banish"], json!(["rider"]));
    assert_eq!(after["P1"]["drive"], json!([]));
    assert_eq!(after["P1"]["evolve_deck"], json!(["point"]));
    assert_eq!(after["objects"]["point"]["face_up"], true);
}
