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
fn observed_literal_name_exclusion_keeps_the_source_name_and_excludes_only_the_written_name() {
    let mut document = tree();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"]["from"] =
        json!({"in":"seen","except":{"in":"seen","name":"unit-amulet"}});
    let programs = lowerer().lower(&document).unwrap();
    let mut game = position();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private filtered Look");
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the observed card identities remain private"
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("literal-name candidates");
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the literal excluded name is filtered"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("unit-follower"),
        "sharing the source name does not exclude the legal card"
    );
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("observed remainder order");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private remainder order");
    };
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["same"]),
        "the legal source-name card reaches hand"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["other"]),
        "the excluded literal-name card stays in the deck"
    );
}

#[test]
fn literal_name_exclusion_does_not_open_other_receipts_or_public_and_union_selectors() {
    let exclusion = json!({"in":"seen","name":"unit-amulet"});
    for from in [
        json!({"in":"seen","except":{"in":"different","name":"unit-amulet"}}),
        json!({"in":"seen","except":{"in":"seen","name":""}}),
        json!({"in":"seen","except":{"in":"seen","names":["unit-amulet"]}}),
        json!({"in":"seen","except":{"in":"seen","name":"unit-amulet","kind":"follower"}}),
        json!({"who":"self","zone":"field","kind":"follower","except":exclusion}),
        json!({"in":"seen","not_same_name_as_this":true,"except":exclusion}),
        json!({"any_of":[{"in":"seen","except":exclusion},{"in":"seen","name":"unit-follower"}]}),
    ] {
        let mut document = tree();
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"]["from"] =
            from;
        assert!(
            lowerer().lower(&document).is_err(),
            "literal exclusion remains confined to one unchanged private Look receipt"
        );
    }
    let mut document = tree();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] = json!([{
        "bind":"formal","from":{"who":"self","zone":"field","kind":"follower","except":exclusion},"n":1_u8
    }]);
    assert!(
        lowerer().lower(&document).is_err(),
        "formal public targets cannot silently ignore a private Look exclusion"
    );
}

#[test]
fn literal_name_exclusion_is_rejected_in_grouped_choices_and_searches() {
    let mut document = tree();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(3);
    let choice =
        &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"];
    choice.as_object_mut().unwrap().remove("n");
    choice["upto"] = json!(2_u8);
    choice["from"] = json!({"in":"seen"});
    choice["groups"] = json!([{"kind":"follower"},{"kind":"amulet"}]);
    lowerer().lower(&document).unwrap();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"]["from"]["except"] =
        json!({"in":"seen","name":"unit-amulet"});
    assert!(
        lowerer().lower(&document).is_err(),
        "an otherwise admitted grouped choice rejects literal exclusion"
    );
    let mut search = tree();
    search["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([{
        "search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u8,"to":"hand"}
    }]);
    lowerer().lower(&search).unwrap();
    search["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"]["sel"]["except"] =
        json!({"in":"seen","name":"unit-amulet"});
    assert!(
        lowerer().lower(&search).is_err(),
        "an otherwise admitted whole-deck search cannot borrow the Look-only exclusion"
    );
}

fn literal_trait_position(program: &WellFormedCardProgram) -> Game {
    let (snapshot, registry, mut cards) = catalog_documents(&json!("1"));
    let mut faces = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for face in &mut faces {
        face["faces"][0]["traits"] = json!(["かげろう"]);
        if face["number"] != "unit-spell" {
            face["faces"][0]["card_type"] = json!("フォロワー");
            face["faces"][0]["power"] = json!("2");
            face["faces"][0]["hp"] = json!("3");
        }
    }
    let mut wrong = faces[0].clone();
    wrong["number"] = json!("wrong-trait");
    wrong["faces"][0]["name"] = json!("wrong-trait");
    wrong["faces"][0]["card_type"] = json!("フォロワー");
    wrong["faces"][0]["traits"] = json!(["別の特徵"]);
    faces.push(wrong);
    let mut parsed = serde_json::from_str::<Value>(&cards).unwrap();
    parsed["cards"]["wrong-trait"] = json!({"status":"complete","abilities":[]});
    cards = parsed.to_string();
    let snapshot = faces
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("look.yaml".into(), cards)]).unwrap(),
    );
    let mut initial = setup(
        1,
        json!([
            {"id":"same","card":"unit-follower"},{"id":"excluded","card":"unit-amulet"},{"id":"wrong","card":"wrong-trait"},{"id":"wrong-kind","card":"unit-spell"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "literal-look",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-amulet", "unit-spell", "wrong-trait"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn observed_literal_exclusion_combines_with_kind_and_trait_and_preserves_remainder_order() {
    let mut document = tree();
    let steps = &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    steps[0]["look"]["top"]["n"] = json!(4_u8);
    steps[1]["choose"]["from"] = json!({"in":"seen","kind":"follower","trait":"かげろう","except":{"in":"seen","name":"unit-amulet"}});
    let programs = lowerer().lower(&document).unwrap();
    let mut game = literal_trait_position(&programs["unit-follower"]);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private matching candidates");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("filtered Look domain");
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "kind, trait and literal-name constraints all apply"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("unit-follower"),
        "only the legal source-name follower remains"
    );
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("three-card remainder order");
    };
    let InputDomain::Order(remainder) = &order.domain else {
        panic!("private remainder");
    };
    assert_eq!(
        remainder.len(),
        3_usize,
        "the literal-excluded, wrong-trait and wrong-kind cards all remain"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(remainder.iter().rev().map(|member| member.handle).collect()),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["same"]),
        "the selected eligible follower reaches hand"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["wrong-kind", "wrong", "excluded"]),
        "the chooser controls the order of the complete remainder"
    );
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
fn source_name_exclusion_reaches_public_choices_and_rejects_other_private_reads() {
    let mut public = tree();
    public["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"choose":{"bind":"picked","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
        {"destroy":"picked"}
    ]);
    lowerer().lower(&public).unwrap();
    public["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["choose"]["from"]["not_same_name_as_this"] =
        json!(true);
    // "これと同名を除く" on public objects compares against the holder's own card names.
    lowerer().lower(&public).unwrap();
    public["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["choose"]["from"]["leader_or"] =
        json!(true);
    assert!(
        lowerer().lower(&public).is_err(),
        "a leader recipient has no card name to compare"
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
