use super::*;

#[test]
fn chain_lightning_alias_replays_from_ex_then_each_own_copy_expires_at_end() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"bolt","card":"PR-411"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"mage1","card":"BP02-046"},{"id":"mage2","card":"BP08-042"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"other","card":"EBD02-019"}]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opposing","card":"BP12-050"}]);
    let mut game = game(&initial);
    for pp in [7_u8, 4] {
        resolved(
            game.play_internal_card("bolt", PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], pp);
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("bolt"))
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["resolution"], json!([]));
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        12_u8
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    for remaining in [2_usize, 1] {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("EX phase trigger");
        };
        assert_eq!(options.len(), remaining);
        assert_eq!(question.actor, PlayerId { seat: 1 });
        let automatic = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        assert!(
            automatic
                .progress
                .facts
                .iter()
                .all(|fact| !matches!(fact, Fact::CardPlayed { .. }))
        );
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 2_usize);
    assert_eq!(state["P2"]["ex"], json!(["opposing"]));
    assert_eq!(state["P2"]["leader"]["life"], 12_u8);
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}
