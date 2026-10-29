use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(step: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let header = &mut card["abilities"][0];
    header["activate"]["cost"] = json!({"pp":0});
    header["activate"]["do"] = Value::Array(vec![step]);
    tree["cards"] = json!({"unit-follower":card});
    tree
}

#[test]
fn effect_evolution_uses_private_matching_resource_and_no_intrinsic_payment() {
    let lowered = lowerer()
        .lower(&tree(json!({"evolve":{"ref":"this"}})))
        .unwrap();
    let base = &lowered["unit-follower"];
    let mut game = evolution::position(1, base, &card(vec![]));
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("evolution form");
    };
    assert!(game.internal_decision(View::P2).is_none());
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("form domain");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-form"));
    let atomic = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(format!("{game:?}"), atomic);
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap_err();
    assert_eq!(format!("{game:?}"), atomic);
    let activation = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        1
    );
    assert!(
        !activation
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["s"]["power"], 4);
    assert_eq!(after["objects"]["s"]["hp"], 5);
    assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
    assert_eq!(after["P1"]["ep"], before["P1"]["ep"]);
    assert_eq!(after["P1"]["sep"], before["P1"]["sep"]);
}

#[test]
fn effect_evolution_rejects_foreign_subject_named_override_and_forged_private_domain() {
    for step in [
        json!({"evolve":{"ref":"enemy"}}),
        json!({"evolve":{"ref":"this","into":"other"}}),
    ] {
        lowerer().lower(&tree(step)).unwrap_err();
    }
    let lowered = lowerer()
        .lower(&tree(json!({"evolve":{"ref":"this"}})))
        .unwrap();
    let valid = lowered["unit-follower"].card();
    for zone in [ZoneKind::Hand, ZoneKind::Deck, ZoneKind::Field] {
        let mut forged = valid.clone();
        let Node::Control {
            control: Control::Sequence { body },
        } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("projection");
        };
        let Node::Decision { decision } = &mut body.steps[0].node else {
            panic!("decision");
        };
        let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
            panic!("objects");
        };
        let Selector::Zone { zone: selected, .. } = &mut constraint.groups[0].selector else {
            panic!("zone");
        };
        *selected = zone;
        WellFormedCardProgram::check(forged).unwrap_err();
    }
}

#[test]
fn effect_evolution_does_as_much_as_possible_when_a_later_form_is_unavailable() {
    let mut input = tree(json!({"evolve":{"ref":"this"}}));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"evolve":{"ref":"this"}},{"evolve":{"ref":"this"}}
    ]);
    let lowered = lowerer().lower(&input).unwrap();
    let mut game = evolution::position(1, &lowered["unit-follower"], &card(vec![]));
    let CardPlay::Awaiting(first) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("first evolution");
    };
    let CardPlay::Awaiting(second) = game
        .answer_internal_card(first.actor, first.token, Answer::Targets(vec![0]))
        .unwrap()
    else {
        panic!("second instruction");
    };
    let InputDomain::Targets(domain) = &second.domain else {
        panic!("empty form domain");
    };
    assert_eq!(domain.members.len(), 0);
    assert_eq!(domain.cardinality.minimum, Nat(0));
    let activation = resolved(
        game.answer_internal_card(second.actor, second.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        1
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
        4
    );
}

#[test]
fn effect_evolution_field_count_guard_uses_only_the_controllers_field() {
    for count in [2, 3] {
        let input = tree(
            json!({"if":{"cond":{"cmp":{"left":{"count":{"who":"self","zone":"field"}},"op":"eq","right":count}},"then":[{"evolve":{"ref":"this"}}]}}),
        );
        let lowered = lowerer().lower(&input).unwrap();
        let mut game = evolution::position(1, &lowered["unit-follower"], &card(vec![]));
        let outcome = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        if count == 2 {
            assert!(matches!(outcome, CardPlay::Awaiting(_)));
        } else {
            resolved(outcome);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                2
            );
        }
    }
    lowerer()
        .lower(&tree(
            json!({"if":{"cond":{"entered_from":{"ref":"this","from":"hand"}},"then":[]}}),
        ))
        .unwrap_err();
}
