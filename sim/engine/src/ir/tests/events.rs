use super::*;

fn triggered(pattern: EventPattern) -> Located<CardProgram> {
    let mut seed = card(program(vec![]), "synthetic");
    if let Some(ability) = seed.node.abilities.first_mut() {
        ability.node.kind = AbilityKind::Triggered;
        ability.node.trigger = Some(source_trigger(pattern));
    }
    seed
}
fn equals(operands: ValueEquality) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Equal {
        operands: Box::new(operands),
    }))
}
#[test]
fn own_end_phase_uses_event_player_phase_and_edge() {
    let event = Binding::new(1, ScopeId(1));
    let seed = triggered(EventPattern::Phase {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::PhasePlayer { event })),
                    right: Expr::Literal(PlayerRef::Controller {}),
                })),
                Expr::Compute(Box::new(Predicate::PhaseEquals {
                    left: Expr::Frozen(Box::new(PhaseFields::PhasePhase { event })),
                    right: Expr::Literal(Phase::End),
                })),
                equals(ValueEquality::Edge {
                    left: Expr::Frozen(Box::new(EdgeFields::PhaseEdge { event })),
                    right: Expr::Literal(Edge::Begin),
                }),
            ],
        })),
    });
    for code in [
        "predicate.player-equals",
        "shape.PhaseFields.PhasePhase",
        "shape.EdgeFields.PhaseEdge",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
    assert!(!codes(&seed).contains(&"value.live"));
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
fn attack_damage_can_match_recipient_flags_and_frozen_source() {
    let event = Binding::new(1, ScopeId(1));
    let seed = triggered(EventPattern::Damage {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::RecipientIsLeader {
                    recipient: DamageRecipient::FromExpression {
                        value: Box::new(Expr::Frozen(Box::new(RecipientFields::DamageRecipient {
                            event,
                        }))),
                    },
                    players: controller_set(),
                })),
                equals(ValueEquality::DamageFlags {
                    left: Expr::Frozen(Box::new(DamageFlagsFields::DamageFlags { event })),
                    right: Expr::Literal(DamageFlags::Attack),
                }),
                Expr::Compute(Box::new(Predicate::NameEquals {
                    object: ObjectView::Snapshot {
                        value: Box::new(Expr::Frozen(Box::new(SnapshotFields::DamageSource {
                            event,
                        }))),
                    },
                    name: Expr::Literal(CardName("synthetic".to_owned())),
                })),
            ],
        })),
    });
    for code in [
        "predicate.recipient-leader",
        "shape.DamageFlagsFields.DamageFlags",
        "reference.snapshot",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
}
#[test]
fn stat_facets_and_fusion_materials_have_typed_iteration_bindings() {
    let event = Binding::new(1, ScopeId(1));
    let facet = Binding::new(2, ScopeId(2));
    let stat = triggered(EventPattern::StatChange {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::AnyStatFacet {
            facets: Expr::Frozen(Box::new(FacetFields::StatChangeFacets { event })),
            facet,
            property: Stat::Life,
            predicate: Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Frozen(Box::new(IntFrozen::StatDelta { event: facet })),
                comparison: Comparison::Less,
                right: Expr::Literal(Int(0)),
            })),
        })),
    });
    assert_missing(&stat, "predicate.stat-facet", "synthetic");
    let fused_event = Binding::new(1, ScopeId(1));
    let material = Binding::new(2, ScopeId(2));
    let fused = triggered(EventPattern::Fused {
        binding: fused_event,
        predicate: Expr::Compute(Box::new(Predicate::AnySnapshot {
            snapshots: Expr::Frozen(Box::new(VecRuntimeObjectViewFields::FusedMaterials {
                event: fused_event,
            })),
            object: material,
            predicate: Expr::Compute(Box::new(Predicate::NameEquals {
                object: ObjectView::Snapshot {
                    value: Box::new(Expr::Bound(material)),
                },
                name: Expr::Literal(CardName("synthetic".to_owned())),
            })),
        })),
    });
    assert_missing(&fused, "predicate.snapshot-any", "synthetic");
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
