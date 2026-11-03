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
fn deck_order_results_feed_only_remainders_and_milled_results_only_conditions() {
    for steps in [
        // The reordered cards themselves need a fresh Look before another private order.
        json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"kept","from":{"in":"seen"},"n":1}},
            {"move":{"ref":"kept","to":"deck","position":"top","order":"chooser","result":"placed"}},
            {"move":{"ref":{"in":{"objects_of":"placed","view":"before"}},"to":"cemetery"}}
        ]),
        json!([
            {"move":{"top":{"who":"self","n":1},"to":"cemetery","result":"milled"}},
            {"move":{"ref":{"in":{"objects_of":"milled","view":"before"}},"to":"hand"}}
        ]),
    ] {
        assert!(
            lowerer().lower(&tree(&steps)).is_err(),
            "an order receipt is not a movement input and a milled card is not a movement input"
        );
    }
}

fn milled_condition(source: &Value) -> Value {
    tree(&json!([
        {"move":source},
        {"if":{"cond":{"cmp":{"left":{"count":{"in":{"objects_of":"moved","view":"before"},"not_class":"ウィッチ"}},"op":"ge","right":1_u8}},
            "then":[{"draw":{"n":1_u8}}]}}
    ]))
}

#[test]
fn receipt_card_conditions_read_only_cards_that_left_the_deck() {
    lowerer()
        .lower(&milled_condition(
            &json!({"top":{"who":"self","n":2},"to":"cemetery","result":"moved"}),
        ))
        .unwrap();
    // A card leaving the field may carry modifications that its printing does not show.
    let field = milled_condition(
        &json!({"ref":{"who":"self","zone":"field","kind":"follower"},"to":"cemetery","result":"moved"}),
    );
    assert_eq!(
        lowerer().lower(&field).unwrap_err().code,
        "unsupported_construct"
    );
}

fn order_receipt_steps() -> Value {
    json!([
        {"look":{"top":{"who":"self","n":3},"bind":"seen"}},
        {"choose":{"bind":"kept","from":{"in":"seen"},"n":1}},
        {"move":{"ref":"kept","to":"deck","position":"top","order":"chooser","result":"placed"}},
        {"look":{"top":{"who":"self","n":3},"bind":"fresh"}},
        {"move":{"ref":{"in":"fresh","except":{"in":{"objects_of":"placed","view":"before"}}},
            "to":"deck","position":"bottom","order":"chooser"}}
    ])
}

#[test]
fn order_receipts_cannot_supply_an_order_domain_even_after_a_fresh_look() {
    let mut steps = order_receipt_steps();
    steps[4]["move"]["ref"] = json!({"in":{"objects_of":"placed","view":"before"}});
    assert!(
        lowerer().lower(&tree(&steps)).is_err(),
        "an Order before receipt is only an exclusion, never an Order input"
    );
}

#[test]
fn typed_order_receipts_are_rejected_outside_difference_exclusions() {
    let programs = lowerer().lower(&tree(&order_receipt_steps())).unwrap();
    for union in [false, true] {
        let mut forged = programs["unit-follower"].card().clone();
        let Node::Instruction { instruction } =
            &mut forged.abilities[0].node.body.as_mut().unwrap().steps[4].node
        else {
            panic!("remainder order")
        };
        let Action::Order {
            cards,
            destinations,
            ..
        } = &mut instruction.action
        else {
            panic!("remainder order")
        };
        let Selector::Difference { input, except } = cards else {
            panic!("remainder difference")
        };
        let receipt = *except.clone();
        *cards = if union {
            Selector::Union {
                sets: vec![*input.clone(), receipt],
            }
        } else {
            receipt
        };
        destinations[0].cards = cards.clone();
        let forged = WellFormedCardProgram::check(forged).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "order-receipt",
        )
        .unwrap();
        assert!(
            game.load_internal_card("unit-follower", &forged).is_err(),
            "typed IR cannot bypass the exclusion-only Order receipt contract"
        );
    }
}

#[test]
fn no_expiry_play_discount_is_bound_to_the_successful_ex_generation() {
    for (full, roundtrip) in [(false, false), (false, true), (true, false)] {
        let moved = |id| MovementReceipt::Move {
            receipt: ReceiptReference::new(ReceiptId(id)),
        };
        let after = |id| Selector::MoveAfter {
            receipt: moved(id),
            permission: FollowPermission::SameEffectExplicitReference {},
        };
        let mut reduction = cost::reduce(3, moved(2));
        if let Node::Instruction { instruction } = &mut reduction.node
            && let Action::InstallEffect {
                duration, effect, ..
            } = &mut instruction.action
            && let Effect::CostModifier {
                duration: inner, ..
            } = effect.as_mut()
        {
            duration.end = DurationEnd::NoTemporalExpiry {};
            inner.end = DurationEnd::NoTemporalExpiry {};
        }
        let mut steps = vec![
            movement::put(
                2,
                Selector::Ordered {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Deck,
                    },
                    start: Position::Top {},
                    count: Expr::Literal(Nat(1)),
                    direction: SliceDirection::Forward,
                },
                OwnerZone::Ex,
            ),
            reduction,
        ];
        if roundtrip {
            steps.extend([
                movement::put(4, after(2), OwnerZone::Cemetery),
                movement::put(5, after(4), OwnerZone::Ex),
            ]);
        }
        let program = card(vec![ability(1, steps)]);
        let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
        if full {
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..5)
                    .map(|i| json!({"id":format!("e{i}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-cost-generation",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let result = activate(&mut game, 1);
        assert!(matches!(
            &result.progress.receipts[&ReceiptId(3)],
            InstructionOutcome::Installed(subjects) if subjects.len() == usize::from(!full)
        ));
        if !full {
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            assert!(matches!(
                game.play_internal_card("d", PlayerId { seat: 1 }).unwrap(),
                CardPlay::Entered(_)
            ));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if roundtrip { 0 } else { 2 },
                "only the installed generation receives the reduction from cost 3 to 1"
            );
        }
    }
}

#[test]
fn observed_unions_need_two_filtered_branches_of_the_same_look() {
    lowerer()
        .lower(&observed(&json!({"any_of":[
            {"in":"seen","kind":"spell"},
            {"in":"seen","kind":"follower","trait":"妖精"}
        ]})))
        .unwrap();
    for from in [
        json!({"any_of":[{"in":"seen","kind":"spell"}]}),
        json!({"any_of":[{"in":"seen","kind":"spell"},{"in":"seen"}]}),
        json!({"any_of":[{"in":"seen","kind":"spell"},{"in":"other","kind":"amulet"}]}),
        json!({"any_of":[{"in":"seen","kind":"spell"},{"who":"self","zone":"field","kind":"follower"}]}),
        json!({"any_of":[{"in":"seen","kind":"spell"},{"in":"seen","other":true}]}),
    ] {
        assert!(
            lowerer().lower(&observed(&from)).is_err(),
            "a union cannot widen to the whole Look, another receipt or a public zone"
        );
    }
    let mut grouped = observed(&json!({"any_of":[
        {"in":"seen","kind":"spell"},{"in":"seen","kind":"amulet"}
    ]}));
    let choice =
        &mut grouped["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"];
    choice.as_object_mut().unwrap().remove("n");
    choice["upto"] = json!(2_u8);
    choice["may"] = json!(true);
    choice["groups"] = json!([{"kind":"spell"},{"kind":"amulet"}]);
    assert!(
        lowerer().lower(&grouped).is_err(),
        "per-group limits apply to one Look query, not to a union"
    );
}

#[test]
fn overlapping_observed_union_branches_keep_each_card_once() {
    let programs = lowerer()
        .lower(&tree(&json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"picked","from":{"any_of":[
                {"in":"seen","kind":"follower"},
                {"in":"seen","class":"ニュートラル"}
            ]},"upto":2}},
            {"move":{"ref":"picked","to":"cemetery"}}
        ])))
        .unwrap();
    let mut game = split_game(&programs["unit-follower"]);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("observed union")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("union targets")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "two observed cards each match both branches"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent gets no private choice"
    );
    let handles = domain.members.iter().map(|member| member.handle).collect();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["deck"], json!(["three", "tail"]));
    assert_eq!(view["P1"]["cemetery"], json!(["s", "one", "two"]));
}

fn crowded_look(choose: &Value, destination: &str, deck: Value) -> (Game, Option<DecisionView>) {
    let mut document = tree(&json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":choose},
        {"move":{"ref":"picked","to":destination,"result":"moved"}}
    ]));
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"] = json!({"pp":0});
    let programs = lowerer().lower(&document).unwrap();
    let mut initial = setup(1, deck);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        ["s", "f1", "f2", "f3"]
            .iter()
            .map(|id| json!({"id":id,"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "crowded-look",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let play = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let question = match play {
        CardPlay::Awaiting(q) => Some(q),
        CardPlay::Resolved(_)
        | CardPlay::Entered(_)
        | CardPlay::Cancelled
        | CardPlay::Continued(_) => None,
    };
    (game, question)
}

#[test]
fn grouped_look_to_hand_ignores_field_capacity() {
    let (mut game, q) = crowded_look(
        &json!({
            "bind":"picked","from":{"in":"seen"},"groups":[
                {"kind":"follower","class":"ニュートラル"},
                {"kind":"spell","class":"ニュートラル"}
            ],"upto":2,"may":true
        }),
        "hand",
        json!([{"id":"one","card":"unit-follower"},{"id":"two","card":"unit-spell"}]),
    );
    let q = q.unwrap();
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("grouped hand targets")
    };
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(2)),
        "taking cards into hand needs no field slots"
    );
    let handles = domain.members.iter().map(|member| member.handle).collect();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["one", "two"])
    );
}

#[test]
fn union_look_to_field_offers_only_the_free_field_slot() {
    let (mut game, q) = crowded_look(
        &json!({
            "bind":"picked","from":{"any_of":[
                {"in":"seen","kind":"follower"},
                {"in":"seen","class":"ニュートラル"}
            ]},"upto":2,"may":true
        }),
        "field",
        json!([{"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}]),
    );
    let q = q.unwrap();
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("union field targets")
    };
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one of two chosen cards fits the field"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(view["P1"]["deck"], json!(["two"]));
}

fn flow_look(steps: &Value, field_count: usize) -> (Game, DecisionView) {
    let mut document = tree(steps);
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"] = json!({"pp":0});
    let programs = lowerer().lower(&document).unwrap();
    let mut initial = setup(
        1,
        json!([
            {"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..field_count).map(|i| json!({"id":if i == 0 { "s".to_owned() } else { format!("f{i}") },"card":"unit-follower"})).collect::<Vec<_>>()
    );
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "look-flow").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    let play = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let q = awaiting_any(play).unwrap();
    (game, q)
}

fn awaiting_any(play: CardPlay) -> Option<DecisionView> {
    match play {
        CardPlay::Awaiting(q) => Some(q),
        CardPlay::Resolved(_)
        | CardPlay::Entered(_)
        | CardPlay::Cancelled
        | CardPlay::Continued(_) => None,
    }
}

fn union_choice() -> Value {
    json!({"choose":{"bind":"picked","from":{"any_of":[
        {"in":"seen","kind":"follower"},{"in":"seen","class":"ニュートラル"}
    ]},"upto":2,"may":true}})
}

/// Answers with as many cards as the question offers; a legal answer must never abort.
fn offered_answer(game: &mut Game, q: &DecisionView) -> CardPlay {
    let handles = if let InputDomain::Targets(domain) = &q.domain {
        let limit = usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap();
        domain
            .members
            .iter()
            .take(limit)
            .map(|member| member.handle)
            .collect()
    } else {
        Vec::new()
    };
    game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}

#[test]
fn choice_inside_expanded_if_keeps_field_limit() {
    let (mut game, q) = flow_look(
        &json!([
            {"if":{"cond":{"cmp":{"left":1,"op":"eq","right":1}},"then":[
                {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
                union_choice(),
                {"move":{"ref":"picked","to":"field","result":"moved"}}
            ]}}
        ]),
        4,
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Look targets")
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    resolved(offered_answer(&mut game, &q));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn field_move_of_another_binding_does_not_cap_hand_pick() {
    let (mut game, q) = flow_look(
        &json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            union_choice(),
            {"move":{"ref":"picked","to":"hand","result":"taken"}},
            {"look":{"top":{"who":"self","n":1},"bind":"fresh"}},
            {"choose":{"bind":"later","from":{"in":"fresh"},"n":1}},
            {"move":{"ref":"later","to":"field","result":"entered"}}
        ]),
        4,
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Look targets")
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    let CardPlay::Awaiting(next) = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
        .unwrap()
    else {
        panic!("later binding question")
    };
    resolved(offered_answer(&mut game, &next));
}

#[test]
fn exact_multi_pick_clamps_minimum_with_maximum() {
    let (mut game, q) = flow_look(
        &json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"n":2}},
            {"move":{"ref":"picked","to":"field","result":"moved"}}
        ]),
        4,
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Look targets")
    };
    assert_eq!(domain.cardinality.minimum, Nat(1));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    resolved(offered_answer(&mut game, &q));
}

#[test]
fn single_card_at_full_field_stays_in_deck() {
    let (mut game, q) = flow_look(
        &json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"n":1}},
            {"move":{"ref":"picked","to":"field","result":"moved"}}
        ]),
        5,
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Look targets")
    };
    assert_eq!(domain.cardinality.minimum, Nat(1));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    resolved(offered_answer(&mut game, &q));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(view["P1"]["deck"], json!(["one", "two"]));
}

#[test]
fn multi_card_look_picks_enter_the_field_only_as_the_next_step() {
    let grouped = json!({"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"groups":[
        {"kind":"follower","orig_cost_le":2},{"kind":"follower","orig_cost_le":3}
    ],"upto":2,"may":true}});
    let look = json!({"look":{"top":{"who":"self","n":2},"bind":"seen"}});
    let enter = json!({"move":{"ref":"picked","to":"field","result":"moved"}});
    let later_if = json!({"if":{"cond":{"cmp":{"left":1,"op":"eq","right":1}},"then":[enter]}});
    let mixed = json!({"if":{"cond":{"cmp":{"left":1,"op":"eq","right":0}},"then":[enter],
        "else":[{"move":{"ref":"picked","to":"hand","result":"taken"}}]}});
    let between = json!({"draw":{"n":1}});
    for choice in [union_choice(), grouped] {
        lowerer()
            .lower(&tree(&json!([look, choice, enter])))
            .unwrap();
        for steps in [
            json!([look, choice, later_if]),
            json!([look, choice, mixed]),
            json!([look, choice, between, enter]),
        ] {
            assert!(
                lowerer().lower(&tree(&steps)).is_err(),
                "free field slots read at the question could change before a later entry"
            );
        }
    }
}

#[test]
fn resolution_modes_cannot_bypass_an_outer_multi_card_entry_guard() {
    let grouped = json!({"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"groups":[
        {"kind":"follower","orig_cost_le":2},{"kind":"follower","orig_cost_le":3}
    ],"upto":2,"may":true}});
    let single_group =
        json!({"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"upto":2}});
    for choice in [union_choice(), grouped, single_group] {
        assert!(
            lowerer()
                .lower(&tree(&json!([
                    {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
                    choice,
                    {"either":{"options":[
                        [{"move":{"ref":"picked","to":"field","result":"placed"}}],
                        [{"move":{"ref":"picked","to":"hand","result":"taken"}}]
                    ]}}
                ])))
                .is_err(),
            "an inherited multi-card pick still cannot enter from a later mode"
        );
    }
}

#[test]
fn a_local_multi_card_pick_can_enter_next_inside_a_resolution_mode() {
    lowerer()
        .lower(&tree(&json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"either":{"options":[
                [union_choice(), {"move":{"ref":"picked","to":"field","result":"placed"}}],
                [{"draw":{"n":0}}]
            ]}}
        ])))
        .unwrap();
}

#[test]
fn optional_local_multi_card_names_do_not_restrict_later_single_picks() {
    lowerer()
        .lower(&tree(&json!([
            {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
            {"optional":{"do":[union_choice(), {"draw":{"n":0}}]}},
            {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"n":1}},
            {"draw":{"n":0}},
            {"move":{"ref":"picked","to":"field","result":"placed"}}
        ])))
        .unwrap();
}

#[test]
fn an_intervening_entry_is_rejected_before_a_multi_card_look_entry() {
    let mut document = tree(&json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        union_choice(),
        {"move":{"ref":"incoming","to":"field","result":"revived"}},
        {"move":{"ref":"picked","to":"field","result":"placed"}}
    ]));
    let activation = &mut document["cards"]["unit-follower"]["abilities"][0]["activate"];
    activation["cost"] = json!({"pp":0});
    activation["targets"] = json!([
        {"bind":"incoming","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}
    ]);
    assert!(
        lowerer().lower(&document).is_err(),
        "field capacity cannot change between the Look choice and its entry"
    );
}
