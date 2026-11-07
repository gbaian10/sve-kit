use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(steps: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0_u8},"do":steps}}),
        "unit-amulet",
    )
}

#[test]
fn author_shuffle_shapes_are_closed_and_do_not_expose_unused_variants() {
    let valid = tree(&json!([
        {"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck"}},
        {"shuffle":{"who":"self"}}, {"look":{"top":{"who":"self","n":2_u8},"bind":"seen"}}
    ]));
    let programs = lowerer().lower(&valid).unwrap();
    assert!(
        matches!(&programs["unit-amulet"].card().abilities[0].node.body.as_ref().unwrap().steps[1].node,
        Node::Instruction {instruction} if matches!(instruction.action,Action::Shuffle {scope:ShuffleScope::FullZone {},..}))
    );
    for steps in [
        json!([{"shuffle":{"who":"opp"}}]),
        json!([{"shuffle":{"who":"self","result":"saved"}}]),
        json!([{"shuffle":{"who":"self","precondition":true}}]),
        json!([{"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck"}}]),
        json!([{"move":{"ref":{"who":"self","zone":"cemetery"},"to":"deck","result":"saved"}},{"shuffle":{"who":"self"}}]),
        json!([{"move":{"ref":{"who":"opp","zone":"cemetery"},"to":"deck"}},{"shuffle":{"who":"self"}}]),
        json!([{"move":{"ref":{"who":"self","zone":"hand"},"to":"deck"}},{"shuffle":{"who":"self"}}]),
        json!([{"look":{"top":{"who":"self","n":3_u8},"bind":"seen"}},{"move":{"ref":{"in":"seen"},"to":"deck","position":"top","order":"shuffle"}}]),
        json!([{"look":{"top":{"who":"self","n":3_u8},"bind":"seen"}},{"move":{"ref":{"in":"seen"},"to":"deck","position":"bottom","order":"shuffle","result":"saved"}}]),
    ] {
        lowerer().lower(&tree(&steps)).unwrap_err();
    }
}

#[test]
fn cutthroat_fanfare_fragment_moves_one_card_to_hand_and_randomizes_only_the_looked_remainder() {
    // The real card also needs an unadmitted deck-construction exception; this admits no real CardId.
    let input = tree(&json!([
        {"look":{"top":{"who":"self","n":8_u8},"bind":"seen"}},
        {"choose":{"from":{"in":"seen"},"n":1_u8,"bind":"picked"}},
        {"move":{"ref":"picked","to":"hand","result":"taken"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"taken","view":"before"}}},"to":"deck","position":"bottom","order":"shuffle"}}
    ]));
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let deck = (0_u8..10)
        .map(|i| json!({"id":format!("d{i}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    let mut initial = setup(1, json!(deck));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    let random =
        json!({"shuffles":[{"player":"P1","result":["d7","d6","d5","d4","d3","d2","d0"]}]});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &random,
        "cutthroat-fragment",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Look choice");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("one of the eight observed cards");
    };
    assert_eq!(domain.members.len(), 8);
    let result = resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[1].handle]),
        )
        .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Shuffled { .. })),
        "a partial shuffle is not a full-deck shuffle"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["d1"]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["d8", "d9", "d7", "d6", "d5", "d4", "d3", "d2", "d0"])
    );
    assert!(
        game.internal_decision(View::P1).is_none(),
        "random placement never asks for a chooser order"
    );
}
