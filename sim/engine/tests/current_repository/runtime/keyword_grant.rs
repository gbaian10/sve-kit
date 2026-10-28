use super::*;

#[test]
fn bp01_047_grants_assail_to_one_other_follower_and_enables_standing_targets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"provider","card":"BP01-047"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let attack = json!({"do":"attack","attacker":"ally","target":"enemy"});
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "cannot-attack"
    );
    assert!(matches!(
        game.play_internal_card("provider", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare");
    };
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("other follower");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("objects");
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["ally"]["keywords"], json!(["指定攻撃"]));
    assert_eq!(view["objects"]["provider"]["keywords"], json!([]));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "resolved"
    );
}
