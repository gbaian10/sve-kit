#![expect(
    clippy::panic,
    reason = "Unexpected IR or question shape invalidates this boundary regression."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn tree() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"self_to_cemetery":true},"do":[
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen","not_same_name_as_this":true},"n":1,"may":true}},
        {"move":{"ref":"picked","to":"hand","result":"moved"}},
        {"reveal":{"ref":"picked","bind":"shown"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}}
    ]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn position() -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"same","card":"unit-follower"},{"id":"other","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "look-source",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn observed_source_names_survive_the_source_being_sacrificed_as_a_cost() {
    let programs = lowerer().lower(&tree()).unwrap();
    let mut game = position();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private Look")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed candidates")
    };
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"]),
        "the source has left the field before its names filter runs"
    );
    assert_eq!(
        domain.members.len(),
        1_usize,
        "sealed source names exclude another physical card with the source name"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("unit-amulet"),
        "a differently named observed card remains eligible after source departure"
    );
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("remainder order")
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private order")
    };
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["other"]),
        "the different-name choice reaches hand"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["same"]),
        "the excluded same-name card remains in the ordered deck"
    );
}

fn unseal_names(program: &mut CardProgram) {
    let Node::Decision { decision } =
        &mut program.abilities[0].node.body.as_mut().unwrap().steps[1].node
    else {
        panic!("choice")
    };
    let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
        panic!("object domain")
    };
    let Selector::Filter { predicate, .. } = &mut constraint.groups[0].selector else {
        panic!("receipt filter")
    };
    let Expr::Compute(and) = predicate.as_mut() else {
        panic!("conjunction")
    };
    let Predicate::And { predicates } = and.as_mut() else {
        panic!("conjunction")
    };
    let Expr::Compute(not) = &mut predicates[1] else {
        panic!("name exclusion")
    };
    let Predicate::Not { predicate } = not.as_mut() else {
        panic!("name exclusion")
    };
    let Expr::Compute(names) = predicate else {
        panic!("names")
    };
    let Predicate::NamesIntersect { right, .. } = names.as_mut() else {
        panic!("names")
    };
    *right = Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::SourceHolder {}),
        },
    }));
}

#[test]
fn observed_names_do_not_admit_an_unsealed_live_source_holder() {
    let programs = lowerer().lower(&tree()).unwrap();
    let mut forged = programs["unit-follower"].card().clone();
    unseal_names(&mut forged);
    let forged = WellFormedCardProgram::check(forged).unwrap();
    let mut game = position();
    let before = exact_authority(&game);
    assert!(
        game.load_internal_card("unit-follower", &forged).is_err(),
        "the private Look checker accepts sealed source information, never an ambient live source holder"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "a rejected source reader preserves authority"
    );
}

#[test]
fn source_name_exclusion_stays_scoped_to_observations_and_rejects_other_private_reads() {
    let mut public = tree();
    public["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"choose":{"bind":"picked","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
        {"destroy":"picked"}
    ]);
    lowerer().lower(&public).unwrap();
    public["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["choose"]["from"]["not_same_name_as_this"] =
        json!(true);
    assert!(
        lowerer().lower(&public).is_err(),
        "the new observed source-name filter does not broaden public field selection"
    );
    for filter in [
        json!({"in":"seen","hp_le":1}),
        json!({"in":"seen","has_evolve":true}),
        json!({"in":"seen","who":"opp"}),
    ] {
        let mut invalid = tree();
        invalid["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"]["from"] =
            filter;
        assert!(
            lowerer().lower(&invalid).is_err(),
            "unrelated private stat, ability and foreign-zone queries stay unsupported"
        );
    }
}
