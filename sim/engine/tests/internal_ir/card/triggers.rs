use super::*;
use sve_engine::game::internal::{Answer, DecisionView, InputDomain};
use sve_engine::game::player::Viewer;

pub(super) fn lastword(mut ability: Located<Ability>) -> Located<Ability> {
    ability.node.kind = AbilityKind::Triggered;
    ability.node.play = None;
    ability.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::Lastword,
        origin: Box::new(ability.node.source.origin.clone()),
    };
    let event = Binding::new(90, ability.node.scope);
    ability.node.trigger = Some(TriggerSpec {
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::BeforeExit,
        aggregation: TriggerAggregation::PerFact {},
        limit: None,
        on: TriggerCondition::Event {
            pattern: EventPattern::LeaveField {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![
                        Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::LeaveFieldObjectBefore {
                                event,
                            })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        })),
                        Expr::Compute(Box::new(Predicate::ZoneEquals {
                            left: Expr::Frozen(Box::new(ZoneFrozen::LeaveFieldTo { event })),
                            right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                                player: PlayerRef::Owner {},
                                kind: ZoneKind::Cemetery,
                            })),
                        })),
                    ],
                })),
            },
        },
    });
    locate_steps(
        &mut ability.node.body.as_mut().unwrap().steps,
        &ability.source,
        &ability.node.source,
    );
    ability
}

fn question(game: &mut Game) -> DecisionView {
    game.internal_pending_choice().unwrap().unwrap()
}

fn choose(game: &mut Game, question: &DecisionView, handle: u64) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn loaded(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "pending-seed",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn activated(initial: &Value, program: &WellFormedCardProgram, seat: u32) -> Game {
    let mut game = loaded(initial, program);
    activate(&mut game, seat);
    game
}

#[test]
fn bp01_p25_lowered_lastword_survives_non_destroy_source_cost() {
    let cards = lowerer().lower(fanfare::expanded::tree()).unwrap();
    let mut program = cards["BP01-P25"].card().clone();
    let mut sacrifice = payment::paid_ability(vec![], vec![]);
    sacrifice.node.id = AbilityId(2);
    sacrifice.node.scope = ScopeId(2);
    sacrifice.node.body.as_mut().unwrap().scope = ScopeId(2);
    sacrifice.source = program.abilities[0].source.clone();
    sacrifice.source.line = 2;
    sacrifice.node.source = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Printed {
            face: FaceId("base".into()),
            line: Nat(2),
        },
    };
    let Node::Instruction { instruction } = movement::put(
        100,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Cemetery,
    )
    .node
    else {
        panic!("move")
    };
    let mut cost = *instruction;
    cost.source = sacrifice.node.source.clone();
    cost.context = InstructionContext::Cost;
    sacrifice.node.play.as_mut().unwrap().costs = CostSpec::Ordered {
        steps: vec![CostStep {
            keyword: None,
            instruction: Box::new(cost),
            materials: None,
        }],
    };
    program.abilities.push(sacrifice);
    let program = WellFormedCardProgram::check(program).unwrap();
    for seat in [1, 2] {
        let mut game = fanfare::position_for_card(seat, &program, "BP01-P25", 20);
        game.play_internal_card("s", PlayerId { seat }).unwrap();
        game.resume_internal_card().unwrap();
        let activation = resolved(
            game.activate_internal_card("s", AbilityId(2), PlayerId { seat })
                .unwrap(),
        );
        assert!(activation.progress.facts.iter().any(
            |fact| matches!(fact, Fact::Movement(movement) if movement.method == MoveMethod::Put)
        ));
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.resume_internal_card().unwrap();
        let question = question(&mut game);
        let result = resolved(choose(&mut game, &question, 1));
        assert_eq!(result.source.holder, activation.source.holder);
        assert_ne!(result.source.instance, activation.source.instance);
        assert!(
            result
                .progress
                .facts
                .iter()
                .filter_map(|fact| if let Fact::Damage(hit) = fact {
                    Some(hit)
                } else {
                    None
                })
                .all(|hit| hit.source.object == result.source.holder && hit.amount.0.get() == 2)
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 18);
        assert_eq!(view["P2"]["leader"]["life"], 18);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn pending_order_keeps_departed_source_lki_and_checks_defeat_before_the_other_player() {
    let damage = source_information::source_hit(
        3,
        source_information::source_read(
            &ObjectView::SourceInformation {},
            CharacteristicView::Effective,
            Some(Stat::Attack),
        ),
    );
    let lethal = instruction(
        2,
        Action::Damage {
            recipients: RecipientSet::Union {
                sets: vec![
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 50),
                    },
                    RecipientSet::Objects {
                        objects: Selector::Zone {
                            players: players(PlayerRelation::Opponents),
                            zone: ZoneKind::Field,
                            view: SelectionView::Current {},
                            binding: Binding::new(60, ScopeId(60)),
                            predicate: Box::new(Expr::Literal(Bool(true))),
                        },
                    },
                ],
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(4)),
            },
            source: ObjectView::SourceInformation {},
            flags: DamageFlags::Ability,
            batch: BatchPolicy::Instruction {},
        },
    );
    let program = card(vec![
        ability(1, vec![numeric::buff(1, 2, 0), lethal]),
        lastword(ability(2, vec![damage])),
        lastword(ability(3, vec![draw(4, 1)])),
    ]);
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([{"id":"d","card":"unit-spell"}]));
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][opponent]["leader"]["life"] = json!(3);
        // An evolved holder's printed abilities come from its form: no base/form Lastword guess.
        let mut evolved = initial.clone();
        evolved["players"][opponent]["zones"]["field"][0]["state"] = json!({"evolved_with":"e"});
        evolved["players"][opponent]["zones"]["evolution"] =
            json!([{"id":"e","card":"unit-follower"}]);
        let mut game = loaded(&evolved, &program);
        let before = exact_authority(&game);
        let error = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert!(format!("{error:?}").contains("evolution information association"));
        assert_eq!(before, exact_authority(&game));
        let mut game = activated(&initial, &program, seat);
        game.resume_internal_card().unwrap();
        let first = question(&mut game);
        assert_eq!(first.actor, PlayerId { seat });
        assert!(matches!(&first.domain, InputDomain::Pending(options) if options.len() == 2));
        let before = exact_authority(&game);
        game.answer_internal_card(PlayerId { seat: 3 - seat }, first.token, Answer::Pending(1))
            .unwrap_err();
        game.answer_internal_card(first.actor, first.token, Answer::Pending(9))
            .unwrap_err();
        game.answer_internal_card(first.actor, first.token, Answer::Integer(Nat(1)))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let draw = resolved(choose(&mut game, &first, 2));
        assert_eq!(draw.source.definition, AbilityId(3));
        game.resume_internal_card().unwrap();
        let second = question(&mut game);
        assert_eq!(second.actor, first.actor);
        assert_ne!(second.token, first.token);
        let before = exact_authority(&game);
        game.answer_internal_card(first.actor, first.token, Answer::Pending(1))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let damage = resolved(choose(&mut game, &second, 1));
        assert!(damage.progress.facts.iter().any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 4 && hit.source.object == damage.source.holder)));
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert_eq!(game.internal_card_stage(), None);
        game.answer_internal_card(first.actor, first.token, Answer::Pending(1))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap()["game"]["ended"],
            true
        );
    }
}

fn lethal_rule_batch(maximum: u64) -> Game {
    let programs = lowerer()
        .lower(&evolution_linked_movement::tree(&[(
            "auto",
            json!({
                "per_turn":maximum,
                "on":{"left_field":{"who":"self","sel":{"kind":"follower"}}},
                "do":[{"draw":{"who":"self","n":1}}]
            }),
        )]))
        .unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    raw.abilities.push(ability(
        20,
        vec![damage(
            20,
            RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 50),
            },
            99,
        )],
    ));
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = setup(
        1,
        json!([{"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-spell"}]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},
        {"id":"other1","card":"unit-other-follower"},
        {"id":"other2","card":"unit-other-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        event_ability::title_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "rule-quota",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-other-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    resolved(
        game.activate_internal_card("s", AbilityId(20), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game
}

#[test]
fn lethal_rule_batch_offers_quota_before_pending_after_the_observer_leaves() {
    for maximum in [1_u64, 2] {
        let mut game = lethal_rule_batch(maximum);
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::Pending));
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap_err();
        let mut stale = None;
        for ordinal in 0..maximum {
            let q = game
                .internal_pending_choice()
                .unwrap()
                .expect("rule-generated quota must remain reachable");
            assert_eq!(game.internal_pending_choice().unwrap(), Some(q.clone()));
            let InputDomain::TriggerQuota(domain) = &q.domain else {
                panic!("quota before pending");
            };
            assert_eq!(
                domain.candidates.len(),
                usize::try_from(3 - ordinal).unwrap()
            );
            assert_eq!(domain.earlier.len(), usize::try_from(ordinal).unwrap());
            let observation = game.observe(Viewer::P1).unwrap();
            assert_eq!(
                observation
                    .pending
                    .iter()
                    .filter(|pending| pending.admission_remaining == Some(maximum - ordinal))
                    .count(),
                usize::try_from(3 - ordinal).unwrap()
            );
            assert!(
                game.observe(Viewer::P2)
                    .unwrap()
                    .pending
                    .iter()
                    .all(|pending| pending.admission_remaining.is_none())
            );
            let before = exact_authority(&game);
            for (actor, token, answer) in [
                (PlayerId { seat: 2 }, q.token, Answer::TriggerQuota(1)),
                (q.actor, q.token, Answer::TriggerQuota(0)),
                (q.actor, q.token, Answer::TriggerQuota(4)),
                (q.actor, q.token, Answer::Pending(1)),
                (q.actor, stale.unwrap_or(q.token), Answer::Integer(Nat(1))),
            ] {
                game.answer_internal_card(actor, token, answer).unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            if let Some(token) = stale {
                game.answer_internal_card(q.actor, token, Answer::TriggerQuota(1))
                    .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!([])
            );
            let response = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::TriggerQuota(domain.candidates[0].handle),
                )
                .unwrap();
            assert!(matches!(response, CardPlay::Awaiting(_)));
            stale = Some(q.token);
        }
        for _ in 0..maximum {
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("admitted pending");
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            usize::try_from(maximum).unwrap()
        );
    }
}
