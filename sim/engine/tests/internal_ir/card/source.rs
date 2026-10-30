use super::*;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

pub(super) fn source_read(
    object: &ObjectView,
    view: CharacteristicView,
    property: Option<Stat>,
) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information: property.map_or_else(
            || NumberRead::Cost {
                object: object.clone(),
                view,
            },
            |property| NumberRead::Stats {
                object: object.clone(),
                view,
                property,
            },
        ),
    }))
}

pub(super) fn source_hit(id: u64, amount: Expr<Int>) -> Located<Node> {
    instruction(
        id,
        Action::Damage {
            recipients: RecipientSet::Leaders {
                players: players(PlayerRelation::Opponents),
            },
            amount: DamageAmount::Uniform { amount },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        },
    )
}

fn departed_body(read_view: &ObjectView) -> Vec<Located<Node>> {
    let information = ObjectView::SourceInformation {};
    vec![
        source_hit(
            1,
            source_read(
                &information,
                CharacteristicView::Original,
                Some(Stat::Attack),
            ),
        ),
        numeric::buff(2, 2, 1),
        movement::put(
            3,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Cemetery,
        ),
        source_hit(
            4,
            source_read(read_view, CharacteristicView::Effective, Some(Stat::Attack)),
        ),
        source_hit(
            5,
            source_read(&information, CharacteristicView::Original, Some(Stat::Life)),
        ),
        source_hit(
            6,
            source_read(&information, CharacteristicView::Effective, None),
        ),
    ]
}

fn damage_facts(facts: &[Fact]) -> Vec<&DamageEvent> {
    facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn source_information_survives_paid_body_departure_and_preserves_damage_source() {
    // The source attack read follows the BP01-118 damage lowering convention;
    // self-movement and stat changes form the synthetic lifecycle contract.
    for seat in [1, 2] {
        let a = payment::paid_ability(
            vec![payment::pp(1)],
            departed_body(&ObjectView::SourceInformation {}),
        );
        let mut game = position_with_card(seat, &card(vec![a]));
        let result = activate(&mut game, seat);
        let hits = damage_facts(&result.progress.facts);
        assert_eq!(
            hits.iter()
                .map(|hit| hit.amount.0.get())
                .collect::<Vec<_>>(),
            [2, 4, 3, 1]
        );
        assert!(
            hits.iter()
                .all(|hit| hit.source.object == result.source.holder)
        );
        assert_eq!(hits[1].source, hits[2].source);
        assert_eq!(hits[2].source, hits[3].source);
        assert!(hits[0].source.snapshot < hits[1].source.snapshot);
        assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(movement)
            if movement.method == MoveMethod::Put && movement.from.kind == ZoneKind::Field
            && movement.movement.before == result.source.holder
            && movement.movement.after.is_some_and(|after| after.generation != result.source.holder.generation))));
        assert!(matches!(
            result.progress.facts.last(),
            Some(Fact::AbilityResolved { .. })
        ));
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[actor]["pp"]["current"], 1);
        assert_eq!(view[opponent]["leader"]["life"], 10);
        game.resume_internal_card().unwrap();

        let a = payment::paid_ability(
            vec![payment::pp(1)],
            departed_body(&ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            }),
        );
        let mut game = position_with_card(seat, &card(vec![a]));
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }

    // A Spell's source is its Resolution generation; its absent attack is zero by rules.
    let information = ObjectView::SourceInformation {};
    let program = card(vec![play::spell(vec![
        source_hit(
            1,
            source_read(&information, CharacteristicView::Original, None),
        ),
        source_hit(
            2,
            source_read(
                &information,
                CharacteristicView::Effective,
                Some(Stat::Attack),
            ),
        ),
    ])]);
    let mut game = play::position(&program, "hand", 0);
    let result = resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let hits = damage_facts(&result.progress.facts);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].amount.0.get(), 1);
    assert_eq!(hits[0].source.object, result.source.holder);
    assert_eq!(result.source.holder.generation, Generation(1));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 19);
    assert_eq!(view["P1"]["cemetery"], json!(["s"]));
}

#[test]
fn source_information_requires_whole_ability_proof_and_rejects_forged_lki() {
    let information = ObjectView::SourceInformation {};
    let body = departed_body(&information);
    WellFormedProgram::check(super::super::body(1, body.clone())).unwrap_err();
    let a = ability(1, body);
    let mut forged = a.clone();
    if let Node::Instruction { instruction } = &mut forged.node.body.as_mut().unwrap().steps[3].node
        && let Action::Damage { source, .. } = &mut instruction.action
    {
        *source = ObjectView::LastKnown {
            object: Box::new(ObjectOperand::SourceHolder {}),
            reason: LkiReason::SourceDeparted,
        };
    }
    WellFormedCardProgram::check(CardProgram {
        card_play_costs: vec![],
        abilities: vec![forged],
    })
    .unwrap_err();
    let role = |object: ObjectView| {
        let mut role = a.clone();
        role.node.body.as_mut().unwrap().steps = vec![node(Node::Capture {
            capture: Capture::Player {
                at: CaptureTime::AtResolution { scope: ScopeId(1) },
                value: Expr::Live(Box::new(PlayerRead::Role {
                    object,
                    role: Role::Controller,
                })),
                into: CaptureRef {
                    binding: Binding::new(1, ScopeId(1)),
                },
            },
        })];
        WellFormedCardProgram::check(CardProgram {
            card_play_costs: vec![],
            abilities: vec![role],
        })
    };
    role(ObjectView::Current {
        object: Box::new(ObjectOperand::SourceHolder {}),
    })
    .unwrap();
    role(information.clone()).unwrap_err();

    // Play-time captures run before the Spell reaches Resolution, so they get no source producer.
    let at_play = |value: Expr<Int>| {
        let mut spell = play::spell(vec![source_hit(1, Expr::Literal(Int(1)))]);
        spell
            .node
            .play
            .as_mut()
            .unwrap()
            .captures
            .push(Capture::Integer {
                at: CaptureTime::AtPlay {
                    execution: ExecutionRef::Current {},
                },
                value,
                into: CaptureRef {
                    binding: Binding::new(80, ScopeId(1)),
                },
            });
        let mut game = game(1, 0);
        let before = exact_authority(&game);
        let loaded = game.load_internal_card("unit-spell", &card(vec![spell]));
        if loaded.is_err() {
            assert_eq!(exact_authority(&game), before);
        }
        loaded
    };
    at_play(Expr::Literal(Int(1))).unwrap();
    let error = at_play(source_read(
        &information,
        CharacteristicView::Original,
        None,
    ))
    .unwrap_err();
    assert!(
        matches!(&error, sve_engine::EngineFailure::Unsupported(message)
        if message.contains("play-time source information"))
    );
}

#[test]
fn source_name_search_uses_sealed_information_after_body_departure() {
    let filter = Expr::Compute(Box::new(Predicate::Not {
        predicate: Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound {
                        binding: Binding::new(1, ScopeId(2)),
                    }),
                },
            })),
            right: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::SourceInformation {},
            })),
        })),
    }));
    let search = search::search(filter, search::cardinality(1, 1), search::hand());
    let program = card(vec![ability(
        1,
        vec![
            movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            ),
            search,
        ],
    )]);
    let mut state = setup(
        1,
        json!([{"id":"same","card":"unit-follower"},{"id":"other","card":"unit-spell"}]),
    );
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "source-names-lki",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let play = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("search input")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>(),
        ["unit-spell"]
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    resolved(play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["P1"]["hand"], json!(["other"]));
}
