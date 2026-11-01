#![expect(clippy::panic, reason = "The scenario requires its printed choice.")]
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected printed choice");
    };
    question
}

#[test]
fn product_machine_pays_with_both_fields_including_itself_before_damage() {
    for number in ["BP07-T01", "BP12-T10", "BP17-T17", "PR-193"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":number},
            {"id":"friend","card":"BP07-110"},
            {"id":"unrelated","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"foreign","card":"BP07-110"},
            {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}
        ]);
        let mut game = game(&initial);
        let target = awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(targets) = &target.domain else {
            panic!("targets");
        };
        let handle = targets
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap()
            .handle;
        let material = awaiting(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![handle]))
                .unwrap(),
        );
        let InputDomain::CostMaterials(materials) = &material.domain else {
            panic!("materials");
        };
        assert_eq!(
            materials.members.len(),
            3,
            "only Machine followers on either field qualify"
        );
        assert_eq!(
            materials
                .members
                .iter()
                .filter(|member| member.player.seat == 2)
                .count(),
            1
        );
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0, 0, 2]),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let result = resolved(
            game.answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![0, 1, 2]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["unrelated"]));
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        let posture = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::Posture(_)))
            .unwrap();
        let played = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .unwrap();
        let damage = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::Damage(_)))
            .unwrap();
        assert!(posture < played && played < damage);
        assert_eq!(state["P1"]["cemetery"], json!(["friend"]));
        assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
        assert!(
            result.progress.facts.iter().all(|fact| !matches!(fact, Fact::Movement(movement) if movement.method != MoveMethod::Put)),
            "material payment never destroys"
        );
    }
}

#[test]
fn product_machine_cannot_pay_with_two_machine_followers_and_an_unrelated_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP07-T01"}, {"id":"friend","card":"BP07-T01"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let target = awaiting(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let error = game
        .answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
        .unwrap_err();
    assert!(matches!(error, EngineFailure::IllegalPlay(_)));
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
