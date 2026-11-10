use super::*;

fn tree() -> Value {
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0_u8},"do":[
            {"reveal_until":{"sel":{"who":"self","zone":"deck","kind":"follower","orig_cost_le":3_u8},"n":2_u8,"bind":"revealed","matched":"matched"}},
            {"move":{"ref":{"in":"matched"},"to":"field","result":"entered"}},
            {"move":{"ref":{"in":"revealed","except":{"in":{"objects_of":"entered","view":"before"}}},"to":"deck","position":"bottom","order":"shuffle"}}
        ]}}),
        "unit-amulet",
    )
}

#[test]
fn revealed_prefix_enters_only_matches_and_shuffles_only_the_actual_remainder() {
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(
        1,
        json!([
            {"id":"miss","card":"unit-amulet"},
            {"id":"one","card":"unit-follower"},
            {"id":"spell","card":"unit-spell"},
            {"id":"two","card":"unit-follower"},
            {"id":"tail","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    let random = json!({"shuffles":[{"player":"P1","result":["spell","miss"]}]});
    let mut game = Game::new(catalog(), &initial, &Value::Null, &random, "reveal-until").unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    resolved(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "one", "two"]));
    assert_eq!(after["P1"]["deck"], json!(["tail", "spell", "miss"]));
    assert!(
        game.internal_decision(View::P1).is_none(),
        "random placement asks for no order"
    );
}

fn prepared(input: &Value, deck: Value, field: Value, random: &Value) -> (Game, AbilityId) {
    let program = lowerer()
        .lower(input)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(1, deck);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = field;
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        random,
        "reveal-boundaries",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    (game, ability)
}

#[test]
fn exhausted_prefixes_with_zero_or_one_match_complete_without_drawing_or_losing() {
    use sve_engine::game::internal::InstructionOutcome;
    for (deck, entered, bottom, matched) in [
        (json!([]), json!(["source"]), json!([]), 0_usize),
        (
            json!([{"id":"miss","card":"unit-amulet"},{"id":"spell","card":"unit-spell"}]),
            json!(["source"]),
            json!(["spell", "miss"]),
            0,
        ),
        (
            json!([{"id":"miss","card":"unit-amulet"},{"id":"one","card":"unit-follower"},{"id":"spell","card":"unit-spell"}]),
            json!(["source", "one"]),
            json!(["spell", "miss"]),
            1,
        ),
        (
            json!([{"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}]),
            json!(["source", "one", "two"]),
            json!([]),
            2,
        ),
    ] {
        let revealed_count = deck.as_array().unwrap().len();
        let random = json!({"shuffles":[{"player":"P1","result":bottom}]});
        let (mut game, ability) = prepared(
            &tree(),
            deck,
            json!([{"id":"source","card":"unit-amulet"}]),
            &random,
        );
        let result = resolved(
            game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert!(
            result.progress.complete,
            "cursor exhaustion is not an empty-deck loss"
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        let outcome = result
            .progress
            .receipts
            .values()
            .find_map(|receipt| {
                if let InstructionOutcome::RevealUntil(outcome) = receipt {
                    Some(outcome)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(outcome.revealed.len(), revealed_count);
        assert_eq!(outcome.matched.len(), matched);
        assert_eq!(outcome.exhausted, Bool(true));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["field"], entered);
        assert_eq!(after["P1"]["deck"], bottom);
    }
}

#[test]
fn a_single_free_space_still_reveals_two_matches_and_returns_the_unentered_match() {
    use sve_engine::game::internal::InputDomain;
    let random = json!({"shuffles":[{"player":"P1","result":["two","spell","miss"]}]});
    let (mut game, ability) = prepared(
        &tree(),
        json!([
            {"id":"miss","card":"unit-amulet"},{"id":"one","card":"unit-follower"},
            {"id":"spell","card":"unit-spell"},{"id":"two","card":"unit-follower"},
            {"id":"tail","card":"unit-follower"}
        ]),
        json!([
            {"id":"source","card":"unit-amulet"},{"id":"f0","card":"unit-follower"},
            {"id":"f1","card":"unit-follower"},{"id":"f2","card":"unit-follower"}
        ]),
        &random,
    );
    let play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("one common movement capacity choice")
    };
    let InputDomain::SearchPartition(domain) = &question.domain else {
        panic!("movement capacity")
    };
    assert_eq!(domain.cards.len(), 2);
    assert_eq!(domain.cardinality.minimum, Nat(1));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    let public = game.projection(View::P2).unwrap();
    for id in ["miss", "one", "spell", "two"] {
        assert!(
            !public["objects"][id].is_null(),
            "the whole prefix is public during the capacity question"
        );
    }
    assert!(
        public["objects"]["tail"].is_null(),
        "the cutoff never reveals the next card"
    );
    let frozen = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::SearchPartition(vec![99]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), frozen);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::SearchPartition(vec![domain.cards[0].handle]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["field"],
        json!(["source", "f0", "f1", "f2", "one"])
    );
    assert_eq!(after["P1"]["deck"], json!(["tail", "two", "spell", "miss"]));
    let hidden = game.projection(View::P2).unwrap();
    for id in ["miss", "spell", "two", "tail"] {
        assert!(
            hidden["objects"][id].is_null(),
            "randomized hidden locations cannot be tracked"
        );
    }
}

#[test]
fn public_deck_revelation_expires_at_resolution_end_even_without_a_following_move() {
    let mut input = tree();
    input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let (mut game, ability) = prepared(
        &input,
        json!([
            {"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"},
            {"id":"tail","card":"unit-follower"}
        ]),
        json!([{"id":"source","card":"unit-amulet"}]),
        &Value::Null,
    );
    resolved(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let public = game.projection(View::P2).unwrap();
    for id in ["one", "two", "tail"] {
        assert!(
            public["objects"][id].is_null(),
            "CR5.21.1.1 restores hidden deck information"
        );
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["one", "two", "tail"])
    );
}

#[test]
fn author_prefix_shapes_reject_foreign_or_dynamic_queries_and_unproved_entry_sets() {
    for (key, value) in [
        ("who", json!("opp")),
        ("zone", json!("hand")),
        ("kind", json!("spell")),
        (
            "orig_cost_le",
            json!({"count":{"who":"self","zone":"cemetery"}}),
        ),
    ] {
        let mut input = tree();
        input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["reveal_until"]["sel"]
            [key] = value;
        lowerer().lower(&input).unwrap_err();
    }
    for (key, value) in [
        ("n", json!(0)),
        ("matched", json!("revealed")),
        ("result", json!("extra")),
        ("precondition", json!(true)),
    ] {
        let mut input = tree();
        input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["reveal_until"][key] =
            value;
        lowerer().lower(&input).unwrap_err();
    }
    let mut unproved = tree();
    unproved["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][1]["move"]["ref"] =
        json!({"in":"revealed"});
    lowerer().lower(&unproved).unwrap_err();
}

#[test]
fn raw_prefixes_cannot_replace_the_actor_deck_count_or_original_cost_predicate() {
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    for change in 0..5 {
        let mut candidate = program.card().clone();
        let body = candidate.abilities[0].node.body.as_mut().unwrap();
        let Node::Instruction { instruction } = &mut body.steps[0].node else {
            panic!("the prefix is one typed instruction")
        };
        let Action::RevealUntil {
            deck,
            required,
            predicate,
            ..
        } = &mut instruction.action
        else {
            panic!("the existing reveal-until action")
        };
        match change {
            0 => *deck = PlayerRef::Owner {},
            1 => *required = Expr::Literal(Nat(0)),
            2 => instruction.actor = PlayerRef::Owner {},
            3 => {
                let Expr::Compute(operation) = predicate else {
                    panic!("fixed predicate")
                };
                let Predicate::And { predicates } = operation.as_mut() else {
                    panic!("kind and cost")
                };
                let Expr::Compute(comparison) = &mut predicates[1] else {
                    panic!("cost condition")
                };
                let Predicate::CompareInt {
                    left: Expr::Compute(number),
                    ..
                } = comparison.as_mut()
                else {
                    panic!("cost comparison")
                };
                let IntOperation::InformationOrZero {
                    information: NumberRead::Cost { view, .. },
                } = number.as_mut()
                else {
                    panic!("original cost")
                };
                *view = CharacteristicView::Effective;
            }
            _ => {
                let Node::Instruction { instruction } = &mut body.steps[1].node else {
                    panic!("field move")
                };
                let Action::Move { objects, .. } = &mut instruction.action else {
                    panic!("field move")
                };
                *objects = Selector::ReceiptMembers {
                    receipt: ObjectReceipt::RevealUntil {
                        receipt: ReceiptReference::new(ReceiptId(u64::MAX)),
                        members: RevealUntilMembers::Matched,
                    },
                    projection: MemberProjection::Before,
                };
            }
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn reveal_receipts_do_not_grant_arbitrary_public_selection_or_move_variants() {
    let mut chooser = tree();
    chooser["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][2]["move"]["order"] =
        json!("chooser");
    lowerer().lower(&chooser).unwrap_err();
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    for union in [false, true] {
        let mut candidate = program.card().clone();
        let Node::Instruction { instruction } =
            &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[1].node
        else {
            panic!("matched entry")
        };
        let Action::Move { objects, .. } = &mut instruction.action else {
            panic!("matched entry")
        };
        if union {
            *objects = Selector::Union {
                sets: vec![objects.clone()],
            };
        } else if let Selector::ReceiptMembers {
            receipt: ObjectReceipt::RevealUntil { members, .. },
            ..
        } = objects
        {
            *members = RevealUntilMembers::Revealed;
        } else {
            panic!("the original field move is one direct matched receipt");
        }
        let checked = WellFormedCardProgram::check(candidate).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "reveal-receipt-boundary",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &checked)
            .unwrap_err();
    }
}

#[test]
fn original_cost_boundary_ignores_effective_cost_changes() {
    let snapshot = [
        ("unit-amulet", "アミュレット", "1"),
        ("cost-three", "フォロワー", "3"),
        ("cost-four", "フォロワー", "4"),
    ]
    .iter()
    .map(|(number, kind, cost)| {
        json!({"number":number,"faces":[{
            "name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,
            "power":if *kind=="アミュレット" {"-"}else{"2"},
            "hp":if *kind=="アミュレット" {"-"}else{"3"},"traits":[],"text":null,"sections":[]
        }]})
        .to_string()
    })
    .collect::<Vec<_>>()
    .join("\n");
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-amulet":{"status":"complete","abilities":[]},
        "cost-three":{"status":"complete","abilities":[]},
        "cost-four":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("boundary.yaml".into(), document)])
            .unwrap(),
    );
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(
        1,
        json!([
            {"id":"four","card":"cost-four","state":{"cost":1_u8}},
            {"id":"first","card":"cost-three","state":{"cost":9_u8}},
            {"id":"second","card":"cost-three","state":{"cost":9_u8}},
            {"id":"tail","card":"cost-four"}
        ]),
    );
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    let random = json!({"shuffles":[{"player":"P1","result":["four"]}]});
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &random,
        "original-cost-boundary",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["cost-three", "cost-four"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    resolved(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["field"],
        json!(["source", "first", "second"]),
        "original three matches even at effective nine"
    );
    assert_eq!(
        after["P1"]["deck"],
        json!(["tail", "four"]),
        "original four never matches even at effective one"
    );
}
