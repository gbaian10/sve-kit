use super::*;

fn triggered(pattern: EventPattern) -> Located<CardProgram> {
    let mut seed = card(program(vec![]), "synthetic");
    if let Some(ability) = seed.node.abilities.first_mut() {
        ability.node.kind = AbilityKind::Triggered;
        ability.node.trigger = Some(source_trigger(pattern));
    }
    seed
}
#[test]
fn played_spell_narrows_the_subject_and_reads_its_event_snapshot() {
    let event = Binding::new(1, ScopeId(1));
    let subject = Binding::new(2, ScopeId(2));
    let seed = triggered(EventPattern::Play {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::PlayedCard {
            subject: Expr::Frozen(Box::new(PlaySubjectFields::PlaySubject { event })),
            card: subject,
            predicate: Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Live(Box::new(KindRead::Kind {
                    object: ObjectView::EventSnapshot {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(Expr::Bound(subject)),
                        }),
                        event: EventContext::Play { event },
                        policy: EventSnapshotPolicy::AtReveal,
                    },
                    view: CharacteristicView::Effective,
                })),
                right: Expr::Literal(CardKind::Spell),
            })),
        })),
    });
    for code in [
        "predicate.played-card",
        "shape.PlaySubjectFields.PlaySubject",
        "value.event-snapshot",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
}
#[test]
fn entry_payment_and_receipt_conditions_have_consumers() {
    let event = Binding::new(1, ScopeId(1));
    let seed = triggered(EventPattern::EnterField {
        binding: event,
        predicate: Expr::Frozen(Box::new(BoolFrozen::PaymentComplete {
            receipt: PaymentReference::Entry {
                receipt: Expr::Frozen(Box::new(EntryPaymentRead::EntryPayment { event })),
                absent: AbsentPayment::NotComplete,
            },
        })),
    });
    assert_missing(&seed, "read.entry-payment", "synthetic");
    assert_missing(&seed, "read.receipt.payment-complete", "synthetic");
    let condition = Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::ReceiptStatusEquals {
                left: Expr::Frozen(Box::new(ReceiptStatusRead::Status {
                    receipt: ReceiptId(1),
                })),
                right: Expr::Literal(ReceiptStatus::Applied),
            })),
            Expr::Compute(Box::new(Predicate::AnyMove {
                moves: Expr::Frozen(Box::new(ReceiptMovesRead::Moves {
                    receipt: MovementReceipt::Move {
                        receipt: ReceiptReference::new(ReceiptId(1)),
                    },
                })),
                movement: Binding::new(2, ScopeId(2)),
                predicate: Expr::Compute(Box::new(Predicate::MoveSucceeded {
                    movement: Binding::new(2, ScopeId(2)),
                })),
            })),
        ],
    }));
    let receipt_seed = card(
        program(vec![Located {
            source: source("synthetic"),
            node: Node::Control {
                control: Control::If {
                    condition,
                    then_body: program(vec![]),
                    else_body: None,
                },
            },
        }]),
        "synthetic",
    );
    assert_missing(&receipt_seed, "predicate.receipt-status", "synthetic");
    assert_missing(&receipt_seed, "predicate.move-any", "synthetic");
}
