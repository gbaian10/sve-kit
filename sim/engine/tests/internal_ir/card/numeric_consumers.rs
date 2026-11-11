//! D02 quantity fragments exercise consumers without admitting additional cards.
use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn bp21_007_modify_fragment_carries_the_ex_layer_through_formal_play() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"buff":{"on":{"who":"self","zone":"ex","kind":"follower"},"power":{"plus":[1,1]},"hp":2}}
        ]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"t","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "EX-modify-fragment",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["t"]["power"], 4);
    assert_eq!(before["objects"]["t"]["hp"], 5);
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.play_internal_card("t", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["t"]["power"], 4);
    assert_eq!(after["objects"]["t"]["hp"], 5);
    assert!(
        after["objects"]["t"]["generation"].as_u64().unwrap()
            > before["objects"]["t"]["generation"].as_u64().unwrap()
    );
    assert_eq!(after["P1"]["ex"], json!([]));
    assert_eq!(after["P1"]["pp"]["current"], 9);
    assert_eq!(after["objects"]["s"]["power"], 2);
}

#[test]
fn bp21_063_modify_fragment_targets_this_after_effect_evolution() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"evolve":{"ref":"this"}},
            {"buff":{"on":"this","power":{"plus":[3,3]},"hp":6}}
        ]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = evolution::position(1, &program, &card(vec![]));
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("effect evolution resource")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("effect evolution choice")
    };
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["s"]["power"], 10,
        "evolved base4, then Add6"
    );
    assert_eq!(after["objects"]["s"]["hp"], 11, "evolved base5, then Add6");
    let generation = after["objects"]["s"]["generation"].as_u64().unwrap();
    assert_eq!(
        generation,
        before["objects"]["s"]["generation"].as_u64().unwrap(),
        "evolution keeps its holder on the field"
    );
    assert!(
        after["objects"]["e"]["generation"].as_u64().unwrap()
            > before["objects"]["e"]["generation"].as_u64().unwrap(),
        "the evolution resource changes zones"
    );
    let changes = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::StatChange(change) = fact {
                Some(change)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 1);
    assert!(
        matches!(changes[0].event.subject,RuntimeHolder::Object {object} if object.generation==Generation(generation))
    );
    assert_eq!(changes[0].event.facets.first.delta, Int(6));
    assert_eq!(
        after["objects"]["u"]["power"], 2,
        "the other follower keeps its original layer"
    );
    assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
    assert_eq!(after["P1"]["ep"], before["P1"]["ep"]);
}
