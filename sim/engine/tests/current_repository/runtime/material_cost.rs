use super::*;

#[test]
fn bp03_118_selects_target_then_sacrifices_one_fallen_angel_before_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"payer","card":"BP03-118"},
        {"id":"material","card":"BP03-118"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let initial_game = game(&initial);
    for selected in [0_u64, 1] {
        let mut game = initial_game.clone();
        let CardPlay::Awaiting(target) = game
            .activate_internal_card("payer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("target");
        };
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let CardPlay::Awaiting(material) = game
            .answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap()
        else {
            panic!("material");
        };
        let InputDomain::CostMaterials(domain) = &material.domain else {
            panic!("materials");
        };
        assert_eq!(domain.members.len(), 2);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0, 0]),
        )
        .unwrap_err();
        assert!(
            game.projection(View::Referee).unwrap() == before,
            "invalid materials preserve pending play"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["payer"]["acted"],
            false
        );
        let result = resolved(
            game.answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![selected]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        if selected == 1 {
            assert_eq!(state["objects"]["payer"]["acted"], true);
        }
        assert_eq!(
            state["P1"]["cemetery"],
            json!([if selected == 0 { "payer" } else { "material" }])
        );
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert!(matches!(
            result.progress.facts.first(),
            Some(Fact::Posture(_))
        ));
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
        assert!(damage > played);
    }
}
