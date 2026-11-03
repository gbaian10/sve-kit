use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn raw_program(count: u64, relation: PlayerRelation) -> CardProgram {
    let spell = play::spell(vec![draw(1, 1)]);
    let source = spell.source.clone();
    let cardinality = Cardinality {
        minimum: Nat(count),
        maximum: Some(Nat(count)),
    };
    let cost = Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::Chosen {
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    selector: Selector::Zone {
                        players: PlayerSet {
                            relative_to: PlayerRef::Controller {},
                            relation,
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: ZoneKind::Hand,
                        view: SelectionView::Current {},
                        binding: Binding::new(300, ScopeId(300)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                    cardinality,
                }],
                cardinality,
                disjoint: Disjointness::Disjoint,
                distinct_by: None,
                aggregate_bounds: vec![],
            },
        },
    };
    CardProgram {
        abilities: vec![spell.clone(), quick::permit()],
        card_play_costs: vec![Located {
            source: source.clone(),
            node: FacePlayCost {
                face: FaceId("0".into()),
                modifiers: vec![],
                exchange: None,
                additional: Some(Located {
                    source,
                    node: PlayCostPayment {
                        scope: ScopeId(300),
                        receipt: ReceiptId(302),
                        play: PlaySpec {
                            limit: None,
                            captures: vec![],
                            parameters: vec![],
                            costs: CostSpec::Ordered {
                                steps: vec![CostStep {
                                    keyword: None,
                                    materials: None,
                                    instruction: Box::new(Instruction {
                                        id: InstructionId(301),
                                        receipt: ReceiptId(301),
                                        actor: PlayerRef::Controller {},
                                        source: spell.node.source,
                                        context: InstructionContext::Cost,
                                        precondition: None,
                                        action: cost,
                                    }),
                                }],
                            },
                        },
                    },
                }),
            },
        }],
    }
}

fn program(count: u64, relation: PlayerRelation) -> WellFormedCardProgram {
    WellFormedCardProgram::check(raw_program(count, relation)).unwrap()
}

fn board(zone: &str, pp: u8, material: bool, quick: bool) -> Game {
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
    if quick {
        initial["turn"]["active"] = json!("P2");
    }
    initial["players"]["P1"]["zones"]["hand"] = if material {
        json!([{"id":"material","card":"unit-follower"}])
    } else {
        json!([])
    };
    if zone == "ex" {
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
    }
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"source","card":"unit-spell"}));
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":2});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "quick-hand",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program(1, PlayerRelation::SelfPlayer))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions fail the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected hand cost");
    };
    question
}

#[test]
fn additional_hand_cost_does_not_pay_itself_and_failure_preserves_authority() {
    for zone in ["hand", "ex"] {
        for (pp, material) in [(0, true), (1, false), (1, true)] {
            let mut game = board(zone, pp, material, false);
            let before = exact_authority(&game);
            let state_before = game.projection(View::Referee).unwrap();
            let played = game.play_internal_card("source", PlayerId { seat: 1 });
            if !material {
                played.unwrap_err();
                assert_eq!(exact_authority(&game), before);
                continue;
            }
            let question = awaiting(played.unwrap());
            let InputDomain::CostMaterials(domain) = &question.domain else {
                panic!("hand cost");
            };
            assert_eq!(domain.members.len(), 1);
            assert!(game.internal_decision(View::P2).is_none());
            let pending = exact_authority(&game);
            for answer in [vec![], vec![0, 0], vec![9]] {
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(answer),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), pending);
            }
            let result = game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            );
            if pp == 0 {
                result.unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(), state_before);
                assert!(game.internal_decision(View::Referee).is_none());
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(), state_before);
            } else {
                resolved(result.unwrap());
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["pp"]["current"], 0);
                assert_eq!(state["P1"]["hand"], json!(["drawn"]));
                assert_eq!(state["P1"]["cemetery"], json!(["material", "source"]));
            }
        }
    }
}

#[test]
fn quick_quote_requires_another_payable_hand_card_without_committing_payment() {
    for zone in ["hand", "ex"] {
        for (pp, material) in [(0, true), (1, false), (1, true)] {
            let mut game = board(zone, pp, material, true);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let before = game.projection(View::Referee).unwrap();
            let question = game.internal_quick_choice().unwrap().unwrap();
            assert_eq!(question.actor, PlayerId { seat: 1 });
            let InputDomain::Quick { options, .. } = &question.domain else {
                panic!("Quick");
            };
            assert_eq!(options.len(), usize::from(pp == 1 && material));
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["hand"], before["P1"]["hand"]);
            assert_eq!(after["P1"]["cemetery"], before["P1"]["cemetery"]);
            assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
            if let Some(option) = options.first() {
                let question = awaiting(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Quick(Some(option.handle)),
                    )
                    .unwrap(),
                );
                resolved(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::CostMaterials(vec![0]),
                    )
                    .unwrap(),
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["hand"],
                    json!(["drawn"])
                );
            }
        }
    }
}

#[test]
fn additional_quick_hand_payment_rejects_wider_raw_ir_shapes() {
    for (count, relation) in [
        (0, PlayerRelation::SelfPlayer),
        (2, PlayerRelation::SelfPlayer),
        (1, PlayerRelation::Opponents),
    ] {
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "hand-shape",
        )
        .unwrap();
        // A foreign hand selector is rejected by WF before runtime admission.
        if let Ok(typed) = WellFormedCardProgram::check(raw_program(count, relation)) {
            game.load_internal_card("unit-spell", &typed).unwrap_err();
        }
    }
    let mut ordinary = program(1, PlayerRelation::SelfPlayer).card().clone();
    ordinary
        .abilities
        .retain(|ability| ability.node.kind == AbilityKind::Spell);
    let ordinary = WellFormedCardProgram::check(ordinary).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "non-quick-hand",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &ordinary)
        .unwrap_err();
}
