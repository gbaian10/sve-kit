use super::*;

pub(super) const fn pp(amount: i64) -> Action {
    Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(amount)),
    }
}

pub(super) fn act() -> Action {
    Action::Posture {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        posture: Posture::Acted,
    }
}

pub(super) fn self_to_cemetery() -> Action {
    Action::Move {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    }
}

pub(super) fn paid_ability(actions: Vec<Action>, body: Vec<Located<Node>>) -> Located<Ability> {
    let mut a = ability(1, body);
    a.node.play.as_mut().unwrap().costs = CostSpec::Ordered {
        steps: actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| {
                let id = u64::try_from(index).unwrap().checked_add(100).unwrap();
                CostStep {
                    instruction: Box::new(Instruction {
                        id: InstructionId(id),
                        actor: PlayerRef::Controller {},
                        source: a.node.source.clone(),
                        precondition: None,
                        context: InstructionContext::Cost,
                        receipt: ReceiptId(id),
                        action,
                    }),
                    materials: None,
                }
            })
            .collect(),
    };
    a
}

fn resource_draw(id: u64) -> Located<Node> {
    draw_value(
        id,
        Expr::Live(Box::new(IntRead::PlayerResource {
            player: PlayerRef::Controller {},
            property: PlayerResource::PpCurrent,
        })),
    )
}

#[test]
fn full_payment_precedes_play_and_body_both_seats() {
    for seat in [1, 2] {
        let a = paid_ability(vec![pp(1), act()], vec![resource_draw(1)]);
        let mut game = position_with_card(seat, &card(vec![a]));
        let result = activate(&mut game, seat);
        cards(&result.progress, 1, 1, 1);
        assert_eq!(
            result.progress.receipts[&ReceiptId(100)],
            InstructionOutcome::Payment {
                quantity: Quantity {
                    amount: Int(1),
                    unit: QuantityUnit::Pp
                },
                subjects: vec![],
            }
        );
        assert_eq!(
            result.progress.receipts[&ReceiptId(101)],
            InstructionOutcome::Payment {
                quantity: Quantity {
                    amount: Int(1),
                    unit: QuantityUnit::Cards
                },
                subjects: vec![result.source.holder],
            }
        );
        assert!(
            matches!(result.progress.facts.as_slice(), [Fact::Posture(event), Fact::AbilityPlayed { execution, source }, Fact::Draw(_), Fact::AbilityResolved { execution: resolved, .. }]
            if event.object == result.source.holder && event.before == Posture::Standing && event.after == Posture::Acted
            && *source == result.source && *execution == result.execution && execution == resolved)
        );
        let actor = if seat == 1 { "P1" } else { "P2" };
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[actor]["pp"]["current"], 1);
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.resume_internal_card().unwrap();
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn partial_cost_prefix_is_restored_on_every_later_failure() {
    for seat in [1, 2] {
        for (actions, state, reason) in [
            (vec![pp(1), act()], json!({"acted":true}), "standing source"),
            (
                vec![act(), pp(3)],
                json!({"acted":false}),
                "insufficient PP",
            ),
            (
                vec![pp(1), act(), act()],
                json!({"acted":false}),
                "standing source",
            ),
            (
                vec![pp(2), pp(1)],
                json!({"acted":false}),
                "insufficient PP",
            ),
        ] {
            let a = paid_ability(actions, vec![draw(1, 1)]);
            let mut game = position_with_state(seat, &card(vec![a]), &state);
            let before = exact_authority(&game);
            let views = [View::P1, View::P2, View::Referee].map(|v| game.projection(v).unwrap());
            let error = game
                .activate_internal_card("s", AbilityId(1), PlayerId { seat })
                .unwrap_err();
            assert!(error.to_string().contains(reason), "{error}");
            assert_eq!(exact_authority(&game), before);
            assert_eq!(
                views,
                [View::P1, View::P2, View::Referee].map(|v| game.projection(v).unwrap())
            );
            assert_eq!(game.internal_card_stage(), None);
        }
    }
}

#[test]
fn zero_and_exact_pp_payments_keep_receipts_and_share_the_instance() {
    for seat in [1, 2] {
        for amount in [0, 1, 2] {
            let a = paid_ability(vec![pp(amount)], vec![]);
            let mut game = position_with_card(seat, &card(vec![a]));
            let first = activate(&mut game, seat);
            assert_eq!(
                first.progress.receipts[&ReceiptId(100)],
                InstructionOutcome::Payment {
                    quantity: Quantity {
                        amount: Int(amount),
                        unit: QuantityUnit::Pp
                    },
                    subjects: vec![],
                }
            );
            assert!(matches!(
                first.progress.facts.first(),
                Some(Fact::AbilityPlayed { .. })
            ));
            game.resume_internal_card().unwrap();
            if amount <= 1 {
                let second = activate(&mut game, seat);
                assert_eq!(first.source.instance, second.source.instance);
                assert_ne!(first.execution, second.execution);
                assert_eq!(
                    second.progress.receipts[&ReceiptId(100)],
                    first.progress.receipts[&ReceiptId(100)]
                );
            }
        }
    }
}

#[test]
fn body_failure_restores_costs_knowledge_events_and_all_producers() {
    for seat in [1, 2] {
        let a = paid_ability(
            vec![act(), pp(1)],
            vec![
                draw(1, 1),
                draw_value(
                    2,
                    Expr::Compute(Box::new(IntOperation::Arithmetic {
                        operation: Arithmetic::Add,
                        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
                    })),
                ),
            ],
        );
        let other = ability(2, vec![draw(3, 1)]);
        let mut game = position_with_card(seat, &card(vec![a, other]));
        let mut untouched = game.clone();
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let retry = game
            .activate_internal_card("s", AbilityId(2), PlayerId { seat })
            .unwrap();
        let baseline = untouched
            .activate_internal_card("s", AbilityId(2), PlayerId { seat })
            .unwrap();
        assert_eq!(retry, baseline);
        assert_eq!(exact_authority(&game), exact_authority(&untouched));
    }
}

#[test]
fn failed_interactive_execution_preserves_preexisting_paid_ability_authority() {
    for seat in [1, 2] {
        let a = paid_ability(vec![pp(1), act()], vec![draw(1, 1)]);
        let mut game = position_with_card(seat, &card(vec![a]));
        let paid = activate(&mut game, seat);
        assert!(paid.progress.complete);
        game.resume_internal_card().unwrap();
        let origin = game.clone();
        let program = checked(vec![up_to(70, 1), draw(71, 10_001)]);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let question = game.internal_decision(View::Referee).unwrap();
        game.answer_internal(
            question.actor,
            question.token,
            Answer::Stepwise(StepwiseAnswer::Continue),
        )
        .unwrap_err();
        transactions::assert_restored(&game, &origin);
        let actor = if seat == 1 { "P1" } else { "P2" };
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["pp"]["current"],
            1
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
            true
        );
    }
}
