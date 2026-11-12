#![expect(
    clippy::panic,
    reason = "Unexpected fixture decisions invalidate the payment proof."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn position(zone: &str, state: &Value, ex: usize) -> Value {
    let mut initial = setup(1, json!([]));
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"] = json!({});
    }
    initial["players"]["P2"]["pp"] = json!({"current":2,"max":2});
    initial["players"]["P2"]["zones"][zone] =
        json!([{"id":"source","card":"unit-follower","state":state}]);
    initial["players"]["P2"]["zones"]["ex"] = json!(
        (0..ex)
            .map(|index| json!({"id":format!("ex-{index}"),"card":"unit-amulet"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn board(initial: &Value, number: &str, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "quick-payment",
    )
    .unwrap();
    let empty = card(vec![]);
    for card_number in ["unit-follower", "unit-amulet", "unit-spell"] {
        game.load_internal_card(
            card_number,
            if card_number == number {
                program
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game
}

fn question(game: &mut Game) -> (DecisionView, Vec<u64>) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick");
    };
    let handles = options.iter().map(|option| option.handle).collect();
    (question, handles)
}

fn unchanged(before: &Value, game: &Game) {
    let after = game.projection(View::Referee).unwrap();
    for key in ["P1", "P2", "objects"] {
        assert_eq!(after[key], before[key], "quoting preserves authority {key}");
    }
}

#[test]
fn ordinary_quick_costs_skip_unpayable_candidates_and_commit_the_same_payment() {
    let counter = |amount| Action::Counter {
        holder: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
        kind: CounterKind("test".into()),
        operation: CounterOperation::Remove {},
        amount: Expr::Literal(Nat(amount)),
    };
    let life = |amount| Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(amount)),
        },
    };
    for (actions, state, payment) in [
        (
            vec![payment::pp(1), payment::pp(1)],
            json!({}),
            Some((0, 20, false, 0)),
        ),
        (vec![payment::pp(1), payment::pp(2)], json!({}), None),
        (
            vec![payment::pp(1), payment::act()],
            json!({"acted":false}),
            Some((1, 20, true, 0)),
        ),
        (
            vec![payment::pp(1), payment::act()],
            json!({"acted":true}),
            None,
        ),
        (
            vec![payment::pp(1), payment::act(), payment::act()],
            json!({}),
            None,
        ),
        (
            vec![payment::pp(1), life(-19)],
            json!({}),
            Some((1, 1, false, 0)),
        ),
        (vec![payment::pp(1), life(-21)], json!({}), None),
        (
            vec![payment::pp(1), counter(2)],
            json!({"counters":{"test":2}}),
            Some((1, 20, false, 0)),
        ),
        (
            vec![payment::pp(1), counter(3)],
            json!({"counters":{"test":2}}),
            None,
        ),
        (
            vec![payment::pp(1), counter(1), counter(2)],
            json!({"counters":{"test":2}}),
            None,
        ),
    ] {
        let payable = payment.is_some();
        let first = quick::quick_activated(payment::paid_ability(actions, vec![]));
        let second = quick::quick_activated(ability(2, vec![]));
        let program = card(vec![first, second]);
        let mut game = board(&position("field", &state, 0), "unit-follower", &program);
        let before = game.projection(View::Referee).unwrap();
        let (q, handles) = question(&mut game);
        assert_eq!(
            handles.len(),
            1 + usize::from(payable),
            "an illegal candidate does not hide the other ability"
        );
        unchanged(&before, &game);
        let quoted = exact_authority(&game);
        question(&mut game);
        assert_eq!(
            exact_authority(&game),
            quoted,
            "a cached quote preserves all authority"
        );
        if let Some((pp, life, acted, counter)) = payment {
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(handles[0])))
                    .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P2"]["pp"]["current"], pp);
            assert_eq!(after["P2"]["leader"]["life"], life);
            assert_eq!(after["objects"]["source"]["acted"], acted);
            assert_eq!(
                after["objects"]["source"]["counters"]["test"]
                    .as_i64()
                    .unwrap_or(0),
                counter
            );
        }
    }
}

#[test]
fn source_move_quote_requires_space_for_the_complete_payment() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"zone":"hand",
        "cost":{"steps":[{"pp":1},{"self_to_ex":true}]},"do":[]});
    tree["cards"] = json!({"unit-follower":seed});
    let lowered = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let program = card(vec![quick::quick_activated(
        lowered.card().abilities[0].clone(),
    )]);
    for ex in [4, 5] {
        let mut game = board(&position("hand", &json!({}), ex), "unit-follower", &program);
        let before = game.projection(View::Referee).unwrap();
        let (q, handles) = question(&mut game);
        unchanged(&before, &game);
        assert_eq!(
            handles.len(),
            usize::from(ex == 4),
            "the source must actually reach EX"
        );
        if ex == 4 {
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(handles[0])))
                    .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P2"]["pp"]["current"], 1);
            assert!(
                after["P2"]["ex"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
        }
    }
}

fn spell(modifier: Expr<Int>) -> WellFormedCardProgram {
    let abilities = vec![play::spell(vec![]), quick::permit()];
    let origin = source("unit-spell");
    WellFormedCardProgram::check(CardProgram {
        abilities,
        card_play_costs: vec![Located {
            source: origin.clone(),
            node: FacePlayCost {
                face: FaceId("0".into()),
                additional: None,
                exchange: None,
                modifiers: vec![Located {
                    source: origin,
                    node: PlayCostModifier {
                        condition: None,
                        change: CostChange::Delta { amount: modifier },
                    },
                }],
            },
        }],
    })
    .unwrap()
}

#[test]
fn spell_quote_uses_resolution_payment_without_committing_the_source_or_modifier() {
    let program = spell(Expr::Literal(Int(1)));
    let mut initial = position("field", &json!({}), 0);
    initial["players"]["P2"]["zones"] = json!({"hand":[{"id":"source","card":"unit-spell"}]});
    let mut game = board(&initial, "unit-spell", &program);
    let before = game.projection(View::Referee).unwrap();
    let (q, handles) = question(&mut game);
    unchanged(&before, &game);
    assert_eq!(handles.len(), 1);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(handles[0])))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P2"]["pp"]["current"], 0,
        "the printed 1 plus the live modifier 1 are paid"
    );
    assert_eq!(after["P2"]["cemetery"], json!(["source"]));
}

#[test]
fn quote_preserves_numeric_faults_and_rolls_back_the_window() {
    let overflow = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Multiply,
        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(2))],
    }));
    for modifier in [overflow, Expr::Literal(Int(i64::MAX))] {
        let program = spell(modifier);
        let mut initial = position("field", &json!({}), 0);
        initial["players"]["P2"]["zones"] = json!({"hand":[{"id":"source","card":"unit-spell"}]});
        let mut game = board(&initial, "unit-spell", &program);
        let before = exact_authority(&game);
        let error = game.internal_quick_choice().unwrap_err();
        assert!(matches!(
            error,
            EngineFailure::Invalid(_) | EngineFailure::Invariant(_)
        ));
        assert_eq!(
            exact_authority(&game),
            before,
            "faults preserve the complete original window authority"
        );
    }
}

#[test]
fn an_x_payment_remains_unsupported_at_the_quick_admission_boundary() {
    let binding = Binding::new(300, ScopeId(1));
    let mut ability = payment::paid_ability(vec![payment::pp(0)], vec![]);
    let play = ability.node.play.as_mut().unwrap();
    play.parameters.push(Decision {
        id: DecisionId(300),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Integer {
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            binding,
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::X,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    });
    let CostSpec::Ordered { steps } = &mut play.costs else {
        panic!("ordered payment");
    };
    let Action::Resource { amount, .. } = &mut steps[0].instruction.action else {
        panic!("PP payment");
    };
    *amount = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Bound(binding),
    }));
    let program = card(vec![quick::quick_activated(ability)]);
    let initial = position("field", &json!({}), 0);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "unsupported-quick-x",
    )
    .unwrap();
    let before = exact_authority(&game);
    assert!(matches!(
        game.load_internal_card("unit-follower", &program),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn source_departure_quotes_preserve_the_existing_zone_and_owner_bounds() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"zone":"hand",
        "cost":{"steps":[{"pp":1},{"discard":{"ref":"this"}}]},"do":[]});
    tree["cards"] = json!({"unit-follower":seed});
    let discard = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let banish = payment::paid_ability(
        vec![
            payment::pp(1),
            Action::Banish {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            },
        ],
        vec![],
    );
    for (zone, kind, discard_payable, banish_payable) in [
        ("hand", ZoneKind::Hand, true, false),
        ("field", ZoneKind::Field, false, true),
        ("cemetery", ZoneKind::Cemetery, false, true),
    ] {
        for owner in ["P1", "P2"] {
            for (mut ability, available, destination) in [
                (
                    discard.card().abilities[0].clone(),
                    discard_payable,
                    "cemetery",
                ),
                (banish.clone(), banish_payable, "banish"),
            ] {
                ability.node.active_zones = vec![kind];
                let program = card(vec![quick::quick_activated(ability)]);
                let mut initial = position(zone, &json!({}), 0);
                initial["players"]["P2"]["zones"][zone][0]["owner"] = json!(owner);
                let mut game = board(&initial, "unit-follower", &program);
                let before = game.projection(View::Referee).unwrap();
                let (q, handles) = question(&mut game);
                unchanged(&before, &game);
                let payable = available && owner == "P2";
                assert_eq!(
                    handles.len(),
                    usize::from(payable),
                    "{zone}/{owner}/{destination}"
                );
                if payable {
                    resolved(
                        game.answer_internal_card(
                            q.actor,
                            q.token,
                            Answer::Quick(Some(handles[0])),
                        )
                        .unwrap(),
                    );
                    let after = game.projection(View::Referee).unwrap();
                    assert_eq!(after["P2"]["pp"]["current"], 1);
                    assert!(
                        after["P2"][destination]
                            .as_array()
                            .unwrap()
                            .contains(&json!("source"))
                    );
                }
            }
        }
    }
}
