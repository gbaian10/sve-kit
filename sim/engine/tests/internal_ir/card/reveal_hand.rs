use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({"cost":{"pp":1},"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    tree
}

#[test]
fn reveal_is_public_during_resolution_and_expiry_preserves_public_movement_and_owner_knowledge() {
    let programs = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(&json!([
            {"reveal":{"ref":{"who":"self","zone":"hand"},"bind":"shown"}},
            {"discard":{"who":"self","n":1}},
            {"discard":{"who":"self","n":1}}
        ])))
        .unwrap();
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent_view = if seat == 1 { View::P2 } else { View::P1 };
        let own_view = if seat == 1 { View::P1 } else { View::P2 };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["pp"] = json!({"current":5_u8,"max":5_u8});
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][actor]["zones"]["hand"] = json!([{"id":"h0","card":"unit-spell"},{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "reveal-discard",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("discard question");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("discard handles");
        };
        assert_eq!(domain.members.len(), 3);
        let during = game.projection(opponent_view).unwrap();
        assert_eq!(during["objects"]["h0"]["card"], "unit-spell");
        assert_eq!(during["objects"]["h1"]["card"], "unit-follower");
        assert!(
            game.internal_decision(opponent_view).is_none(),
            "public reveal does not authorize answering a private discard"
        );
        let CardPlay::Awaiting(next) = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap()
        else {
            panic!("second discard question");
        };
        let middle = game.projection(opponent_view).unwrap();
        assert_eq!(
            middle["objects"]["h1"]["card"], "unit-follower",
            "another card leaving does not relocate this unordered hand member"
        );
        assert_eq!(middle["objects"]["h2"]["card"], "unit-spell");
        resolved(
            game.answer_internal_card(next.actor, next.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let after = game.projection(opponent_view).unwrap();
        assert_eq!(
            after["objects"]["h0"]["card"], "unit-spell",
            "public cemetery movement remains visible"
        );
        assert!(
            after["objects"]["h2"].is_null(),
            "unmoved hand card is concealed at resolution end"
        );
        assert_eq!(
            game.projection(own_view).unwrap()["objects"]["h2"]["card"],
            "unit-spell"
        );
    }
}

#[test]
fn reveal_does_not_admit_filtered_hands_or_turn_the_binding_into_arbitrary_private_reads() {
    for steps in [
        json!([{"reveal":{"ref":{"who":"all","zone":"hand"},"bind":"shown"}}]),
        json!([{"reveal":{"ref":{"who":"opp","zone":"hand","kind":"follower"},"bind":"shown"}}]),
        json!([{"reveal":{"ref":{"who":"opp","zone":"deck"},"bind":"shown"}}]),
        json!([{"reveal":{"ref":{"who":"opp","zone":"hand"},"bind":"shown"}},{"destroy":{"ref":{"who":"opp","zone":"hand"}}}]),
        json!([{"reveal":{"ref":{"who":"opp","zone":"hand"},"bind":"shown"}},{"destroy":{"ref":"shown"}}]),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree(&steps))
            .unwrap_err();
    }
}
