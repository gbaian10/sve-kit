use super::*;

#[test]
fn emilia_selects_once_from_hand_and_ex_then_triggers_for_the_entered_soldier() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"SP01-010"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"hand","card":"BP01-046"},
        {"id":"wrong","card":"SP01-010"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex","card":"BP01-042"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"SP01-010"}]);
    let mut game = game(&initial);
    let source = game.internal_activated_sources("source").unwrap()[0];
    let CardPlay::Awaiting(question) = game
        .activate_internal_card_instance("source", source.instance, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("single hand and EX choice");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("hand and EX candidates");
    };
    assert_eq!(domain.members.len(), 2);
    assert!(game.internal_decision(View::P2).is_none());
    assert!(game.projection(View::P2).unwrap()["objects"]["hand"].is_null());
    let ex = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-042"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![ex]))
            .unwrap(),
    );
    event_cemetery::finish_checks(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("soldier entry observer");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "SP01-010");
    let mut play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(target_question) = play {
        let InputDomain::Targets(target_domain) = &target_question.domain else {
            panic!("enemy follower target");
        };
        assert_eq!(target_domain.members.len(), 1);
        play = game
            .answer_internal_card(
                target_question.actor,
                target_question.token,
                Answer::Targets(vec![target_domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    event_cemetery::finish_checks(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "ex"]));
    assert_eq!(view["P1"]["hand"], json!(["hand", "wrong"]));
    assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(view["P1"]["pp"]["current"], 10_u8);
    game.activate_internal_card_instance("source", source.instance, PlayerId { seat: 1 })
        .unwrap_err();
}
