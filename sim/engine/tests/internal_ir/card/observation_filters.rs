use super::*;
use sve_engine::{
    EngineFailure,
    game::internal::{DecisionView, InputDomain},
};

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"self_to_cemetery":true},"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn observed(filter: &Value) -> Value {
    tree(&json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":filter,"n":1}},
        {"move":{"ref":"picked","to":"cemetery"}}
    ]))
}

#[test]
fn observed_remainders_reject_foreign_receipts_and_movement_receipts_as_the_input() {
    for foreign in [true, false] {
        let mut steps = json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"picked","from":{"in":"seen"},"n":1,"may":true}},
            {"move":{"ref":"picked","to":"hand","result":"moved"}},
            {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"cemetery"}}
        ]);
        lowerer().lower(&tree(&steps)).unwrap();
        if foreign {
            steps[2] = json!({"destroy":{"ref":"this","result":"moved"}});
        } else {
            steps[3]["move"]["ref"] = json!({"in":{"objects_of":"moved","view":"before"}});
        }
        assert!(
            lowerer().lower(&tree(&steps)).is_err(),
            "only a Look input and a successful Move exclusion authorize the remainder"
        );
    }
}

#[test]
fn move_deck_orders_a_chosen_observed_subset_without_changing_generations() {
    let document = tree(&json!([
        {"look":{"top":{"who":"self","n":3},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2,"may":true}},
        {"move":{"ref":"picked","to":"deck","position":"bottom","order":"chooser"}}
    ]));
    let programs = lowerer().lower(&document).unwrap();
    let mut initial = setup(
        1,
        json!([
            {"id":"one","card":"unit-follower"}, {"id":"two","card":"unit-follower"},
            {"id":"tail","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "chosen-order",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private subset")
    };
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![0, 1]))
        .unwrap()
    else {
        panic!("private subset order")
    };
    let InputDomain::Order(domain) = &order.domain else {
        panic!("order of the chosen subset")
    };
    assert_eq!(domain.len(), 2);
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(order.actor, order.token, Answer::Order(vec![1, 0]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["deck"], json!(["tail", "two", "one"]));
    assert_eq!(view["objects"]["one"]["generation"], 0_u8);
    assert_eq!(view["objects"]["two"]["generation"], 0_u8);
}

#[test]
fn observation_filters_lower_class_not_class_and_basic_keywords_through_the_shared_filter() {
    for filter in [
        json!({"in":"seen","class":"ドラゴン"}),
        json!({"in":"seen","not_class":"ドラゴン"}),
        json!({"in":"seen","keyword":"ward"}),
    ] {
        lowerer().lower(&observed(&filter)).unwrap();
    }
    for filter in [
        json!({"in":"seen","keyword":"single_drive"}),
        json!({"in":"seen","keyword":"unknown"}),
        json!({"in":"seen","who":"opp"}),
        json!({"in":"unbound","kind":"follower"}),
    ] {
        assert!(
            lowerer().lower(&observed(&filter)).is_err(),
            "unsupported keyword and unauthorized receipt filters are rejected"
        );
    }
}

#[test]
fn keyword_counts_and_exists_remain_public_and_leader_keywords_are_not_ignored() {
    for zone in ["field", "cemetery", "ex"] {
        lowerer().lower(&tree(&json!([
            {"if":{"cond":{"exists":{"who":"self","zone":zone,"keyword":"ward"}},"then":[{"pp":{"who":"self","delta":1}}]}},
            {"if":{"cond":{"cmp":{"left":{"count":{"who":"self","zone":zone,"keyword":"ward"}},"op":"ge","right":1}},"then":[{"pp":{"who":"self","delta":1}}]}}
        ]))).unwrap();
    }
    for zone in ["hand", "deck"] {
        assert!(lowerer().lower(&tree(&json!([{"if":{"cond":{"cmp":{"left":{"count":{"who":"self","zone":zone,"keyword":"ward"}},"op":"ge","right":1}},"then":[{"pp":{"who":"self","delta":1}}]}}]))).is_err(), "keyword counts do not expose hidden zones");
    }
    assert!(
        lowerer()
            .lower(&tree(&json!([
                {"choose":{"bind":"picked","from":{"leader":"opp","keyword":"ward"},"n":1}},
                {"damage":{"to":"picked","n":1}}
            ])))
            .is_err(),
        "recipient lowering cannot silently discard an object keyword filter"
    );
}

#[test]
fn private_observation_bounds_do_not_authorize_public_or_opponent_decisions() {
    let programs = lowerer()
        .lower(&observed(&json!({"in":"seen","kind":"follower"})))
        .unwrap();
    for public in [true, false] {
        let mut forged = programs["unit-follower"].card().clone();
        let Node::Decision { decision } =
            &mut forged.abilities[0].node.body.as_mut().unwrap().steps[1].node
        else {
            panic!("observation decision")
        };
        if public {
            decision.visibility = Visibility::Public {};
        } else {
            decision.actor = PlayerRef::Active {};
        }
        let Ok(forged) = WellFormedCardProgram::check(forged) else {
            continue;
        };
        let initial = setup(1, json!([]));
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "private-observation",
        )
        .unwrap();
        assert!(
            game.load_internal_card("unit-follower", &forged).is_err(),
            "only a controller's private receipt decision is admitted"
        );
    }
}

#[test]
fn optional_single_observation_group_can_choose_two_cards() {
    let mut tree = observed(&json!({"in":"seen","kind":"follower"}));
    let choice = &mut tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"];
    choice.as_object_mut().unwrap().remove("n");
    choice["upto"] = json!(2_u8);
    choice["may"] = json!(true);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][2]["move"]["result"] =
        json!("moved");
    let programs = lowerer().lower(&tree).unwrap();
    for zero in [true, false] {
        let mut invalid = programs["unit-follower"].card().clone();
        let Node::Decision { decision } =
            &mut invalid.abilities[0].node.body.as_mut().unwrap().steps[1].node
        else {
            panic!("observation choice")
        };
        let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
            panic!("object domain")
        };
        if zero {
            let bounds = Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(0)),
            };
            constraint.cardinality = bounds;
            constraint.groups[0].cardinality = bounds;
            let DecisionKind::ResolutionChoice { cardinality, .. } = &mut decision.kind else {
                panic!("resolution bounds")
            };
            *cardinality = bounds;
        } else {
            constraint.groups[0].cardinality.maximum = Some(Nat(1));
        }
        assert!(
            WellFormedCardProgram::check(invalid).is_err(),
            "exact zero retains its distinct contract and optional group bounds must match the decision"
        );
    }
    let mut state = setup(1, json!([]));
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    state["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "two-observed",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("observation")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed targets")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        },
        "IR and runtime accept the same optional two-card bounds"
    );
    assert_eq!(
        domain.members.len(),
        2_usize,
        "both looked followers are eligible"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["source", "one", "two"]),
        "the common movement consumer accepts both selected receipt members"
    );
}

#[test]
fn private_look_does_not_authorize_destroying_deck_members() {
    let tree = tree(&json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"n":1,"may":true}},
        {"destroy":"picked"}
    ]));
    let Ok(programs) = lowerer().lower(&tree) else {
        return;
    };
    let mut state = setup(1, json!([]));
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    state["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "destroy-observed",
    )
    .unwrap();
    let before = exact_authority(&game);
    if game
        .load_internal_card("unit-follower", &programs["unit-follower"])
        .is_err()
    {
        assert_eq!(
            exact_authority(&game),
            before,
            "static rejection preserves authority"
        );
        return;
    }
    let before = game.projection(View::Referee).unwrap();
    let opponent_before = game.projection(View::P2).unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("observed choice")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed members")
    };
    let error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
    assert!(
        matches!(error,EngineFailure::Unsupported(message) if message.contains("non-public movement origin")),
        "a receipt authorizes Put movement, never destroying a deck member"
    );
    let after = game.projection(View::Referee).unwrap();
    let opponent_after = game.projection(View::P2).unwrap();
    assert_eq!(
        after["objects"], before["objects"],
        "physical generations are restored"
    );
    assert_eq!(
        opponent_after["objects"], opponent_before["objects"],
        "private observation never leaks"
    );
    for player in ["P1", "P2"] {
        assert_eq!(
            after[player], before[player],
            "the rejected activation restores its source cost and never moves a deck card"
        );
        assert_eq!(
            opponent_after[player], opponent_before[player],
            "rejecting a deck destruction never exposes the private observation"
        );
    }
}

#[test]
fn keyword_identifier_admission_does_not_authorize_ambient_ability_reads() {
    let programs=lowerer().lower(&tree(&json!([
        {"if":{"cond":{"exists":{"who":"self","zone":"field","keyword":"ward"}},"then":[{"pp":{"who":"self","delta":1}}]}}
    ]))).unwrap();
    let mut state = setup(1, json!([]));
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut allowed = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "public-keyword",
    )
    .unwrap();
    allowed
        .load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let mut forged = programs["unit-follower"].card().clone();
    let Node::Control {
        control: Control::If { condition, .. },
    } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("condition")
    };
    *condition = Expr::Compute(Box::new(Predicate::HasAbility {
        abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            view: CharacteristicView::Effective,
        })),
        predicate: Box::new(AbilityPredicate {
            kind: None,
            keyword: Some(KeywordId::Ward),
            except: vec![],
            holder: None,
            origin: None,
        }),
    }));
    let forged = WellFormedCardProgram::check(forged).unwrap();
    let mut game = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "ambient-keyword",
    )
    .unwrap();
    let before = exact_authority(&game);
    let error = game
        .load_internal_card("unit-follower", &forged)
        .unwrap_err();
    assert!(
        matches!(error,EngineFailure::Unsupported(message) if message.contains("authorized selector spine")),
        "identifier admission never permits a free source-holder ability read"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "rejected ambient readers preserve authority"
    );
}

fn split_game(program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(
        1,
        json!([
            {"id":"one","card":"unit-follower"}, {"id":"two","card":"unit-follower"},
            {"id":"three","card":"unit-follower"}, {"id":"tail","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "split").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game
}

fn awaiting_order(play: CardPlay) -> Option<DecisionView> {
    match play {
        CardPlay::Awaiting(q) if matches!(q.domain, InputDomain::Order(_)) => Some(q),
        CardPlay::Awaiting(_)
        | CardPlay::Resolved(_)
        | CardPlay::Entered(_)
        | CardPlay::Cancelled
        | CardPlay::Continued(_) => None,
    }
}

#[test]
fn chosen_top_and_unchosen_remainder_bottom_reuse_one_look() {
    let programs = lowerer()
        .lower(&tree(&json!([
            {"look":{"top":{"who":"self","n":3},"bind":"seen"}},
            {"choose":{"bind":"kept","from":{"in":"seen"},"upto":3}},
            {"move":{"ref":"kept","to":"deck","position":"top","order":"chooser","result":"placed"}},
            {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"placed","view":"before"}}},
                "to":"deck","position":"bottom","order":"chooser"}}
        ])))
        .unwrap();
    // Choosing nothing skips the empty top block and still orders every observed card.
    for (kept, top, bottom, deck) in [
        (
            vec![0, 2],
            vec![1, 0],
            vec![0],
            json!(["three", "one", "tail", "two"]),
        ),
        (
            vec![],
            vec![],
            vec![2, 1, 0],
            json!(["tail", "three", "two", "one"]),
        ),
    ] {
        let mut game = split_game(&programs["unit-follower"]);
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("private subset")
        };
        let mut next = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(kept.clone()))
            .unwrap();
        if !kept.is_empty() {
            let first = awaiting_order(next).unwrap();
            next = game
                .answer_internal_card(first.actor, first.token, Answer::Order(top))
                .unwrap();
        }
        let second = awaiting_order(next).unwrap();
        assert!(
            game.internal_decision(View::P2).is_none(),
            "the opponent gets no private order"
        );
        resolved(
            game.answer_internal_card(second.actor, second.token, Answer::Order(bottom))
                .unwrap(),
        );
        assert_eq!(game.projection(View::Referee).unwrap()["P1"]["deck"], deck);
    }
}

#[test]
fn deck_order_results_feed_only_remainders_and_top_results_only_ex() {
    for steps in [
        // The reordered cards themselves need a fresh Look before another private order.
        json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"kept","from":{"in":"seen"},"n":1}},
            {"move":{"ref":"kept","to":"deck","position":"top","order":"chooser","result":"placed"}},
            {"move":{"ref":{"in":{"objects_of":"placed","view":"before"}},"to":"cemetery"}}
        ]),
        json!([{"move":{"top":{"who":"self","n":1},"to":"cemetery","result":"milled"}}]),
    ] {
        assert!(
            lowerer().lower(&tree(&steps)).is_err(),
            "an order receipt is not a movement input and a milled card has no followable output"
        );
    }
}
