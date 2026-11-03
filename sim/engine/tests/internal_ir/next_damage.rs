#![expect(
    clippy::panic,
    reason = "Unexpected replacement questions fail the scenario."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

const fn own_leader() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::One,
    }
}

fn install(id: u64, reduction: bool) -> Located<Node> {
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let recipient = Binding::new(1, ScopeId(id.checked_add(10).unwrap()));
    let mut step = instruction(
        id,
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(0)),
            mode: StepwiseMode::Exact,
        },
    );
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("installation instruction")
    };
    instruction.action = Action::InstallEffect {
        subjects: InstallationSubjects::Players {
            players: own_leader(),
            affected: Binding::new(1, ScopeId(id.checked_add(20).unwrap())),
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::Replacement {
            id: EffectDefinitionId(id),
            source: instruction.source.clone(),
            pattern: ProposedEventPattern::Damage {
                recipient: Some(RecipientPredicate {
                    binding: recipient,
                    condition: Expr::Compute(Box::new(Predicate::RecipientIsLeader {
                        recipient: DamageRecipient::Bound { binding: recipient },
                        players: own_leader(),
                    })),
                }),
                source: None,
                kind: DamageKind::Any,
            },
            condition: Expr::Literal(Bool(true)),
            rewrite: if reduction {
                EventRewrite::AddDamage {
                    amount: Expr::Literal(Int(-5)),
                }
            } else {
                EventRewrite::PreventDamage {
                    recipient: None,
                    source: None,
                    kind: DamageKind::Any,
                }
            },
            scope: ReplacementScope::General,
            affected_player: PlayerRef::ProposedDamageRecipient {},
            once_per_event: OncePerEvent::Enforced,
            usage: Some(UseLimit {
                count: Nat(1),
                scope: UsageScope::SharedPlayer {
                    player: PlayerRef::Controller {},
                    effect: EffectReference::CurrentInstallation {},
                },
            }),
            prospective_chars: None,
            duration: duration.clone(),
        }),
        duration,
    };
    step
}

fn own_damage(amount: i64) -> WellFormedProgram {
    checked(vec![damage(
        100,
        RecipientSet::Leaders {
            players: own_leader(),
        },
        amount,
    )])
}

fn perform(game: &mut Game, seat: u32, amount: i64, reverse: bool) -> Progress {
    let first = game
        .execute_internal(&own_damage(amount), &source("synthetic"), context(seat))
        .unwrap();
    if first.complete {
        return first;
    }
    let question = game.internal_decision(View::Referee).unwrap();
    assert_eq!(
        question.actor,
        PlayerId { seat },
        "The affected player chooses replacement order"
    );
    let InputDomain::DamageReplacements(options) = &question.domain else {
        panic!("replacement order")
    };
    assert_eq!(
        options.len(),
        2,
        "Both independent installations remain candidates"
    );
    assert!(
        options
            .iter()
            .all(|option| option.card == "unit-spell" && option.name == "unit-spell"),
        "Choice labels retain the provider public face"
    );
    let handles = options
        .iter()
        .map(|option| option.handle)
        .collect::<Vec<_>>();
    let before = exact_authority(game);
    game.answer_internal(
        PlayerId {
            seat: if seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Order(handles.clone()),
    )
    .unwrap_err();
    assert_eq!(
        before,
        exact_authority(game),
        "A wrong player cannot change authority or budgets"
    );
    game.answer_internal(
        question.actor,
        question.token,
        Answer::Order(vec![handles[0], handles[0]]),
    )
    .unwrap_err();
    assert_eq!(
        before,
        exact_authority(game),
        "Duplicate handles cannot change authority or budgets"
    );
    let order = if reverse {
        handles.into_iter().rev().collect()
    } else {
        handles
    };
    game.answer_internal(question.actor, question.token, Answer::Order(order))
        .unwrap()
}

fn install_pair(game: &mut Game, seat: u32, second_reduction: bool) {
    game.execute_internal(
        &checked(vec![install(1, true), install(2, second_reduction)]),
        &source("synthetic"),
        context(seat),
    )
    .unwrap();
}

#[test]
fn replacement_order_preserves_unused_prevention_or_reduction() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for amount in [3, 7] {
            for reverse in [false, true] {
                let mut game = game(seat, 0);
                install_pair(&mut game, seat, false);
                let first = perform(&mut game, seat, amount, reverse);
                assert!(first.complete);
                assert!(
                    !first
                        .facts
                        .iter()
                        .any(|fact| matches!(fact, Fact::Damage(_)))
                );
                perform(&mut game, seat, 7, false);
                let loss = if reverse {
                    2
                } else if amount == 3 {
                    0
                } else {
                    7
                };
                assert_eq!(
                    game.projection(View::Referee).unwrap()[actor]["leader"]["life"],
                    20 - loss
                );
            }
        }
    }
}

#[test]
fn two_reductions_consume_only_while_positive_damage_remains() {
    for amount in [3, 12] {
        let mut game = game(1, 0);
        install_pair(&mut game, 1, true);
        perform(&mut game, 1, amount, false);
        let first_loss = if amount == 12 { 2 } else { 0 };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20 - first_loss
        );
        perform(&mut game, 1, 7, false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20 - first_loss - if amount == 12 { 7 } else { 2 }
        );
    }
}

#[test]
fn zero_damage_neither_asks_for_order_nor_consumes_either_installation() {
    let mut game = game(1, 0);
    install_pair(&mut game, 1, false);
    assert!(perform(&mut game, 1, 0, false).complete);
    perform(&mut game, 1, 7, true);
    perform(&mut game, 1, 7, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18
    );
}

#[test]
fn failed_later_instruction_restores_consumed_budget_and_damage() {
    let mut game = game(1, 0);
    game.execute_internal(
        &checked(vec![install(1, true)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let before = game.clone();
    let mut body = own_damage(7).program().steps.clone();
    body.push(damage(101, recipients(), i64::MAX));
    game.execute_internal(&checked(body), &source("synthetic"), context(1))
        .unwrap_err();
    transactions::assert_restored(&game, &before);
    perform(&mut game, 1, 7, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18
    );
}

#[test]
fn setting_leader_life_does_not_use_the_next_damage_reduction() {
    let mut game = game(1, 0);
    game.execute_internal(
        &checked(vec![
            install(1, true),
            instruction(
                2,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Set {
                        amount: Expr::Literal(Int(10)),
                    },
                },
            ),
        ]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    perform(&mut game, 1, 7, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        8
    );
}
