#![expect(
    clippy::panic,
    reason = "Unexpected policy choices fail these small public-data fixtures."
)]

use super::*;
use crate::player_pipeline::question;
use std::sync::OnceLock;
use sve_engine::game::player::observation::{Observation, ObservedZone};
use sve_engine::game::player::{
    AbilityChoice, Answer, CardOption, ClosingAnswer, ClosingQuestion, Command, DecisionShape,
    DiscardKind, DistributionQuestion, EarlierQuotaChoice, EvolutionAnswer, EvolutionPayment,
    EvolutionQuestion, MainIntentKind, MovementCapacityQuestion, OpeningAnswer, OpeningQuestion,
    PartitionQuestion, PendingChoice, QuestionShape, QuestionToken, QuotaQuestion,
    ReplacementChoice, SearchAnswer, SearchQuestion, SelectionGroup, SelectionMember,
    SelectionQuestion, SelectionRole, SubmissionKind, TargetRoleAssignment, TokenCapacityQuestion,
    Viewer,
};
use sve_engine::policy::{BasicPolicy, Choice, PolicyFailure};

fn board(hidden: &str) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-follower"}, {"id":"second","card":"unit-follower"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":format!("private-{hidden}"),"card":hidden}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "basic-policy",
    )
    .unwrap();
    let follower = card(vec![ability(1, vec![draw(1, 0)])]);
    game.load_internal_card("unit-follower", &follower).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

fn observation() -> &'static Observation {
    static OBSERVATION: OnceLock<Observation> = OnceLock::new();
    OBSERVATION.get_or_init(|| board("unit-spell").observe(Viewer::P1).unwrap())
}

fn choose(shape: &QuestionShape) -> Result<Choice, PolicyFailure> {
    BasicPolicy::new(Viewer::P1).choose(observation(), shape)
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "A wrong answer variant is a fixture failure, distinct from the policy errors under test."
)]
fn answer(shape: DecisionShape) -> Result<Answer, PolicyFailure> {
    let Choice::Decision(answer) = choose(&QuestionShape::Decision(shape))? else {
        panic!("decision");
    };
    Ok(answer)
}

const fn bounds(minimum: u64, maximum: u64) -> Cardinality {
    Cardinality {
        minimum: Nat(minimum),
        maximum: Some(Nat(maximum)),
    }
}

fn option(handle: u64) -> CardOption {
    CardOption {
        handle,
        card: "public".into(),
        name: "name".into(),
        object: None,
        rules_names: None,
        original_cost: None,
    }
}

fn selection() -> SelectionQuestion {
    SelectionQuestion {
        members: [9, 2]
            .into_iter()
            .map(|handle| SelectionMember {
                handle,
                player: Viewer::P1,
                card: None,
                name: None,
                object: None,
                rules_names: None,
                original_cost: None,
            })
            .collect(),
        cardinality: bounds(1, 1),
        distinct_names: false,
        original_costs: None,
        distinct_original_costs: false,
        groups: vec![SelectionGroup {
            handles: vec![9, 2],
            cardinality: bounds(1, 1),
        }],
        roles: Vec::new(),
        minimum_original_cost: None,
        maximum_original_cost: None,
    }
}

fn occurrence(handle: u64) -> PendingChoice {
    PendingChoice {
        handle,
        card: "public".into(),
        line: 1,
        event: "enter".into(),
        source: None,
    }
}

fn command(choice: &Choice, token: QuestionToken) -> Command {
    match choice {
        Choice::Decision(answer) => Command::Answer {
            token,
            answer: answer.clone(),
        },
        Choice::Opening(answer) => Command::Opening {
            token,
            answer: answer.clone(),
        },
        Choice::Closing(answer) => Command::Closing {
            token,
            answer: answer.clone(),
        },
        Choice::Evolution(answer) => Command::Evolution {
            token,
            answer: answer.clone(),
        },
        Choice::MainAction { intent, answer } => Command::MainAction {
            token,
            intent: intent.id.clone(),
            answer: answer.clone(),
        },
    }
}

#[test]
fn policy_integer_routes_and_optional_answers_use_the_real_domains() {
    let cases = vec![
        (
            DecisionShape::DiceReroll {
                value: Nat(1),
                remaining: Nat(5),
            },
            Answer::Integer(Nat(0)),
        ),
        (
            DecisionShape::Integer(bounds(7, 9)),
            Answer::Integer(Nat(7)),
        ),
        (
            DecisionShape::Integer(Cardinality {
                minimum: Nat(u64::MAX),
                maximum: None,
            }),
            Answer::Integer(Nat(u64::MAX)),
        ),
        (
            DecisionShape::ExchangeRoutes(vec![Nat(8), Nat(21)]),
            Answer::Integer(Nat(8)),
        ),
        (DecisionShape::May, Answer::Integer(Nat(1))),
        (DecisionShape::OptionalCost, Answer::OptionalCost(true)),
        (DecisionShape::ResolutionPayment, Answer::OptionalCost(true)),
        (
            DecisionShape::BeforeDraw,
            Answer::Stepwise(StepwiseAnswer::Stop),
        ),
        (
            DecisionShape::Quick {
                window: QuickWindow::Attack,
                options: Vec::new(),
            },
            Answer::Quick(None),
        ),
        (
            DecisionShape::SearchReplacement,
            Answer::SearchReplacement(SearchReplacementAnswer::Original),
        ),
        (
            DecisionShape::WardEntry {
                card: "public".into(),
                name: "name".into(),
                zone: "hand".into(),
                position: 1,
                object: None,
            },
            Answer::WardEntry(true),
        ),
    ];
    for (shape, expected) in cases {
        assert_eq!(answer(shape).unwrap(), expected);
    }
    assert_eq!(
        answer(DecisionShape::Integer(bounds(9, 7))),
        Err(PolicyFailure::InvalidQuestion)
    );
    assert_eq!(
        answer(DecisionShape::ExchangeRoutes(Vec::new())),
        Err(PolicyFailure::NoSolution)
    );
}

#[test]
fn policy_occurrences_use_current_coordinates_and_orders_are_complete() {
    let abilities = vec![AbilityChoice {
        handle: 17,
        source_card: "public".into(),
        source_line: 2,
        source_section: None,
        holder: None,
    }];
    assert_eq!(
        answer(DecisionShape::Abilities(abilities)).unwrap(),
        Answer::Ability(17)
    );
    assert_eq!(
        answer(DecisionShape::Pending(vec![occurrence(42), occurrence(6)])).unwrap(),
        Answer::Pending(42)
    );
    let quota = QuotaQuestion {
        candidates: vec![occurrence(42)],
        earlier: vec![EarlierQuotaChoice {
            actor: Viewer::P2,
            occurrence: occurrence(6),
        }],
    };
    assert_eq!(
        answer(DecisionShape::TriggerQuota(quota)).unwrap(),
        Answer::TriggerQuota(42)
    );
    assert_eq!(
        answer(DecisionShape::Order(vec![option(99), option(4)])).unwrap(),
        Answer::Order(vec![99, 4])
    );
    let replacements = [7, 2]
        .into_iter()
        .map(|handle| ReplacementChoice {
            handle,
            card: "public".into(),
            name: "replacement".into(),
            line: None,
            object: None,
        })
        .collect();
    assert_eq!(
        answer(DecisionShape::DamageReplacements(replacements)).unwrap(),
        Answer::Order(vec![7, 2])
    );
    assert_eq!(
        answer(DecisionShape::Order(vec![option(1), option(1)])),
        Err(PolicyFailure::InvalidQuestion)
    );
    for shape in [
        DecisionShape::Abilities(Vec::new()),
        DecisionShape::Pending(Vec::new()),
        DecisionShape::TriggerQuota(QuotaQuestion {
            candidates: Vec::new(),
            earlier: Vec::new(),
        }),
    ] {
        assert_eq!(answer(shape), Err(PolicyFailure::NoSolution));
    }
}

#[test]
fn policy_material_and_target_channels_preserve_roles_and_discard_purpose() {
    for shape in [
        DecisionShape::CostMaterials(selection()),
        DecisionShape::Discard {
            kind: DiscardKind::Cost,
            cards: selection(),
        },
    ] {
        assert_eq!(answer(shape).unwrap(), Answer::CostMaterials(vec![9]));
    }
    for shape in [
        DecisionShape::Targets(selection()),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards: selection(),
        },
    ] {
        assert_eq!(answer(shape).unwrap(), Answer::Targets(vec![9]));
    }
    assert_eq!(
        answer(DecisionShape::ResourceEntryPayment {
            materials: selection(),
            ep_allowed: true
        })
        .unwrap(),
        Answer::ResourceEntryPayment {
            materials: vec![9],
            use_ep: true
        }
    );
    let mut roles = selection();
    roles.roles.push(SelectionRole {
        role: Nat(8),
        position: None,
        cardinality: RoleCardinality::Fixed {
            cardinality: bounds(1, 1),
        },
    });
    assert_eq!(
        answer(DecisionShape::Targets(roles.clone())).unwrap(),
        Answer::TargetsWithRoles {
            targets: vec![9],
            roles: vec![TargetRoleAssignment {
                role: Nat(8),
                targets: vec![9]
            }],
        }
    );
    for shape in [
        DecisionShape::CostMaterials(roles.clone()),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards: roles.clone(),
        },
        DecisionShape::ResourceEntryPayment {
            materials: roles,
            ep_allowed: false,
        },
    ] {
        assert_eq!(answer(shape), Err(PolicyFailure::InvalidQuestion));
    }
}

#[test]
fn policy_search_partition_movement_and_distribution_return_complete_answers() {
    let search = SearchQuestion {
        cards: vec![option(7), option(2)],
        cardinality: bounds(1, 1),
        groups: None,
        distinct_names: false,
        maximum_original_cost: None,
    };
    assert_eq!(
        answer(DecisionShape::Search(search)).unwrap(),
        Answer::Search(SearchAnswer::Ungrouped(vec![7]))
    );
    let partition = PartitionQuestion {
        cards: vec![option(7), option(2)],
        cardinality: bounds(2, 2),
    };
    assert_eq!(
        answer(DecisionShape::SearchPartition(partition.clone())).unwrap(),
        Answer::SearchPartition(vec![7, 2])
    );
    let movement = MovementCapacityQuestion {
        destination: Viewer::P2,
        zone: ObservedZone::Ex,
        available: Nat(2),
        selection: partition,
    };
    assert_eq!(
        answer(DecisionShape::MovementCapacity(movement)).unwrap(),
        Answer::SearchPartition(vec![7, 2])
    );
    let distribution = DistributionQuestion {
        members: selection().members,
        total: Nat(7),
        minimum: Nat(2),
    };
    let Answer::Distribution(shares) = answer(DecisionShape::Distribution(distribution)).unwrap()
    else {
        panic!("distribution");
    };
    assert_eq!(
        shares
            .iter()
            .map(|share| (share.handle, share.amount.0))
            .collect::<Vec<_>>(),
        vec![(9, 5), (2, 2)]
    );
}

#[test]
fn policy_drive_and_token_capacity_use_only_advertised_choices() {
    for icon in [TriggerIcon::Critical, TriggerIcon::Stand] {
        assert_eq!(
            answer(DecisionShape::DriveTrigger {
                icon,
                targets: vec![option(9), option(2)]
            })
            .unwrap(),
            Answer::DriveTrigger {
                accept: true,
                target: Some(9)
            }
        );
        assert_eq!(
            answer(DecisionShape::DriveTrigger {
                icon,
                targets: Vec::new()
            })
            .unwrap(),
            Answer::DriveTrigger {
                accept: true,
                target: None
            }
        );
    }
    for icon in [TriggerIcon::Draw, TriggerIcon::Heal] {
        assert_eq!(
            answer(DecisionShape::DriveTrigger {
                icon,
                targets: Vec::new()
            })
            .unwrap(),
            Answer::DriveTrigger {
                accept: true,
                target: None
            }
        );
    }
    assert_eq!(
        answer(DecisionShape::DriveTrigger {
            icon: TriggerIcon::Front,
            targets: Vec::new()
        })
        .unwrap(),
        Answer::DriveTrigger {
            accept: false,
            target: None
        }
    );
    let tokens = TokenCapacityQuestion {
        destination: Viewer::P1,
        zone: ObservedZone::Field,
        available: Nat(1),
        choices: vec![vec!["a".into()], vec!["b".into()]],
    };
    assert_eq!(
        answer(DecisionShape::TokenCapacity(tokens.clone())).unwrap(),
        Answer::TokenCapacity(0)
    );
    let mut empty = tokens;
    empty.choices.clear();
    assert_eq!(
        answer(DecisionShape::TokenCapacity(empty)),
        Err(PolicyFailure::NoSolution)
    );
}

#[test]
fn policy_modes_preserve_printed_ids_and_native_zero_availability() {
    for shape in [
        DecisionShape::Modes {
            modes: vec![ModeId(9), ModeId(2)],
            cardinality: bounds(1, 2),
            count: Nat(2),
        },
        DecisionShape::ResolutionAlternative {
            modes: vec![ModeId(9), ModeId(2)],
            cardinality: bounds(1, 1),
            count: Nat(1),
        },
    ] {
        assert_eq!(answer(shape).unwrap(), Answer::Modes(vec![ModeId(9)]));
    }
    for (ids, cardinality, count, expected) in [
        (
            vec![ModeId(9)],
            bounds(1, 1),
            0,
            Ok(Answer::Modes(Vec::new())),
        ),
        (Vec::new(), bounds(1, 1), 2, Ok(Answer::Modes(Vec::new()))),
        (
            vec![ModeId(9), ModeId(2)],
            bounds(0, 2),
            1,
            Err(PolicyFailure::NoSolution),
        ),
        (
            vec![ModeId(9)],
            bounds(2, 2),
            1,
            Err(PolicyFailure::NoSolution),
        ),
        (
            vec![ModeId(9), ModeId(9)],
            bounds(1, 1),
            1,
            Err(PolicyFailure::InvalidQuestion),
        ),
        (
            vec![ModeId(9)],
            bounds(0, 0),
            1,
            Ok(Answer::Modes(Vec::new())),
        ),
    ] {
        assert_eq!(
            answer(DecisionShape::Modes {
                modes: ids,
                cardinality,
                count: Nat(count)
            }),
            expected
        );
    }
}

#[test]
fn policy_opening_closing_and_evolution_answers_keep_their_channels() {
    let cards = observation().players[0]
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .map(|card| card.card.clone())
        .collect::<Vec<_>>();
    let handle = cards[0].handle.clone();
    assert_eq!(
        choose(&QuestionShape::Opening(OpeningQuestion::StartAmulet(
            cards.clone()
        )))
        .unwrap(),
        Choice::Opening(OpeningAnswer::StartAmulet(handle.clone()))
    );
    assert_eq!(
        choose(&QuestionShape::Opening(OpeningQuestion::FirstPlayer)).unwrap(),
        Choice::Opening(OpeningAnswer::FirstPlayer(Viewer::P1))
    );
    assert_eq!(
        choose(&QuestionShape::Opening(OpeningQuestion::Mulligan(
            cards.clone()
        )))
        .unwrap(),
        Choice::Opening(OpeningAnswer::Keep)
    );
    assert_eq!(
        choose(&QuestionShape::Closing(ClosingQuestion::GuardAct(
            cards.clone()
        )))
        .unwrap(),
        Choice::Closing(ClosingAnswer::GuardAct(vec![handle.clone()]))
    );
    assert_eq!(
        choose(&QuestionShape::Closing(ClosingQuestion::Discard {
            cards: cards.clone(),
            count: 1
        }))
        .unwrap(),
        Choice::Closing(ClosingAnswer::Discard(vec![handle]))
    );
    assert_eq!(
        choose(&QuestionShape::Closing(ClosingQuestion::Discard {
            cards,
            count: 2
        })),
        Err(PolicyFailure::NoSolution)
    );
    assert_eq!(
        choose(&QuestionShape::Evolution(EvolutionQuestion::Materials(
            selection()
        )))
        .unwrap(),
        Choice::Evolution(EvolutionAnswer::Materials(vec![9]))
    );
    assert_eq!(
        choose(&QuestionShape::Evolution(EvolutionQuestion::Discard(
            selection()
        )))
        .unwrap(),
        Choice::Evolution(EvolutionAnswer::Discard(vec![9]))
    );
    assert_eq!(
        choose(&QuestionShape::Opening(OpeningQuestion::StartAmulet(
            Vec::new()
        ))),
        Err(PolicyFailure::NoSolution)
    );
    assert_eq!(
        choose(&QuestionShape::Evolution(EvolutionQuestion::Form(
            Vec::new()
        ))),
        Err(PolicyFailure::NoSolution)
    );
}

#[test]
fn policy_evolution_payment_uses_pp_before_ep_and_never_overpays() {
    for (total, available, allowed, expected) in [
        (
            2,
            2,
            true,
            Ok(EvolutionAnswer::Payment {
                pp: 2,
                ep: false,
                super_evolution: true,
            }),
        ),
        (
            2,
            1,
            true,
            Ok(EvolutionAnswer::Payment {
                pp: 1,
                ep: true,
                super_evolution: true,
            }),
        ),
        (
            0,
            0,
            true,
            Ok(EvolutionAnswer::Payment {
                pp: 0,
                ep: false,
                super_evolution: true,
            }),
        ),
        (2, 0, true, Err(PolicyFailure::NoSolution)),
        (2, 1, false, Err(PolicyFailure::NoSolution)),
        (-1, 0, true, Err(PolicyFailure::InvalidQuestion)),
        (1, -1, true, Err(PolicyFailure::InvalidQuestion)),
    ] {
        let payment = EvolutionPayment {
            total_pp: total,
            available_pp: available,
            ep_allowed: allowed,
            super_allowed: true,
        };
        assert_eq!(
            choose(&QuestionShape::Evolution(EvolutionQuestion::Payment(
                payment
            ))),
            expected.map(Choice::Evolution)
        );
    }
}

#[test]
fn policy_feedback_spends_only_the_submitted_main_choice_and_rejection_is_retryable() {
    let mut game = board("unit-spell");
    let public = question(&game, Viewer::P1);
    let observation = game.observe(Viewer::P1).unwrap();
    let mut policy = BasicPolicy::new(Viewer::P1);
    let first = policy.choose(&observation, &public.shape).unwrap();
    assert_eq!(policy.choose(&observation, &public.shape).unwrap(), first);
    let wrong = game.submit_player(Viewer::P2, command(&first, public.token));
    assert_eq!(wrong.kind, SubmissionKind::Rejected);
    policy.record_submission(&first, wrong.kind);
    assert_eq!(policy.choose(&observation, &public.shape).unwrap(), first);
    policy.record_submission(&first, SubmissionKind::EngineFault);
    assert_eq!(policy.choose(&observation, &public.shape).unwrap(), first);
    let other = policy
        .choose(&observation, &QuestionShape::Decision(DecisionShape::May))
        .unwrap();
    policy.record_submission(&other, SubmissionKind::Applied);
    assert_eq!(policy.choose(&observation, &public.shape).unwrap(), first);
    let applied = game.submit_player(Viewer::P1, command(&first, public.token));
    assert_eq!(applied.kind, SubmissionKind::Applied);
    policy.record_submission(&first, applied.kind);
    let next = question(&game, Viewer::P1);
    let choice = policy
        .choose(&game.observe(Viewer::P1).unwrap(), &next.shape)
        .unwrap();
    assert!(
        matches!(&choice, Choice::MainAction { intent, .. } if matches!(intent.kind, MainIntentKind::Activate { .. }))
    );
    let applied = game.submit_player(Viewer::P1, command(&choice, next.token));
    assert_eq!(applied.kind, SubmissionKind::Applied);
    policy.record_submission(&choice, applied.kind);
    let next = question(&game, Viewer::P1);
    assert!(
        matches!(policy.choose(&game.observe(Viewer::P1).unwrap(), &next.shape).unwrap(),
        Choice::MainAction { intent, .. } if matches!(intent.kind, MainIntentKind::Attack { .. }))
    );
}

#[test]
fn policy_reverted_and_cancelled_declarations_spend_per_kind_until_public_turn_changes() {
    let game = board("unit-spell");
    let public = question(&game, Viewer::P1);
    let observation = game.observe(Viewer::P1).unwrap();
    for result in [SubmissionKind::Reverted, SubmissionKind::Cancelled] {
        let mut policy = BasicPolicy::new(Viewer::P1);
        let play = policy.choose(&observation, &public.shape).unwrap();
        policy.record_submission(&play, result);
        let activation = policy.choose(&observation, &public.shape).unwrap();
        assert!(
            matches!(&activation, Choice::MainAction { intent, .. } if matches!(intent.kind, MainIntentKind::Activate { .. }))
        );
        policy.record_submission(&activation, result);
        let attack = policy.choose(&observation, &public.shape).unwrap();
        assert!(
            matches!(&attack, Choice::MainAction { intent, .. } if matches!(intent.kind, MainIntentKind::Attack { .. }))
        );
        policy.record_submission(&attack, SubmissionKind::Applied);
        assert_eq!(policy.choose(&observation, &public.shape).unwrap(), attack);
        let mut changed = observation.clone();
        changed.turn.phase = Some(Phase::End);
        policy
            .choose(&changed, &QuestionShape::Decision(DecisionShape::May))
            .unwrap();
        assert_eq!(policy.choose(&observation, &public.shape).unwrap(), attack);
        changed.turn.elapsed_turns[0] = changed.turn.elapsed_turns[0].checked_add(1).unwrap();
        assert_eq!(policy.choose(&changed, &public.shape).unwrap(), play);
    }
    let mut wrong = observation;
    wrong.viewer = Viewer::P2;
    assert_eq!(
        BasicPolicy::new(Viewer::P1).choose(&wrong, &public.shape),
        Err(PolicyFailure::InvalidQuestion)
    );
    wrong.viewer = Viewer::P1;
    wrong.turn.active = Some(Viewer::P2);
    assert_eq!(
        BasicPolicy::new(Viewer::P1).choose(&wrong, &public.shape),
        Err(PolicyFailure::InvalidQuestion)
    );
}

#[test]
fn policy_uses_public_questions_to_finish_normal_evolution() {
    let (base, _) = evolution::programs();
    let mut game =
        evolution::position_with_zones(1, base, &card(vec![]), false, &json!({"P1":{"hand":[]}}));
    game.advance_player_input();
    let mut policy = BasicPolicy::new(Viewer::P1);
    for expected in ["evolve", "form", "payment"] {
        let public = question(&game, Viewer::P1);
        let choice = policy
            .choose(&game.observe(Viewer::P1).unwrap(), &public.shape)
            .unwrap();
        match (expected, &choice) {
            ("evolve", Choice::MainAction { intent, .. }) => {
                assert!(matches!(intent.kind, MainIntentKind::Evolve { .. }));
            }
            ("form", Choice::Evolution(EvolutionAnswer::Form(_))) => {}
            ("payment", Choice::Evolution(EvolutionAnswer::Payment { pp, ep, .. })) => {
                assert_eq!(*pp, 1);
                assert!(!ep);
            }
            _ => panic!("unexpected evolution choice"),
        }
        let submission = game.submit_player(Viewer::P1, command(&choice, public.token));
        assert_eq!(submission.kind, SubmissionKind::Applied);
        policy.record_submission(&choice, submission.kind);
    }
    let public = question(&game, Viewer::P1);
    assert!(!matches!(public.shape, QuestionShape::Evolution(_)));
}

#[test]
fn policy_choices_are_independent_of_opponent_private_cards_and_identifiers() {
    let first = board("unit-spell");
    let second = board("unit-follower");
    let first_observation = first.observe(Viewer::P1).unwrap();
    let second_observation = second.observe(Viewer::P1).unwrap();
    assert_eq!(first_observation, second_observation);
    let first_question = question(&first, Viewer::P1);
    let second_question = question(&second, Viewer::P1);
    assert_eq!(first_question.shape, second_question.shape);
    assert_eq!(
        BasicPolicy::new(Viewer::P1)
            .choose(&first_observation, &first_question.shape)
            .unwrap(),
        BasicPolicy::new(Viewer::P1)
            .choose(&second_observation, &second_question.shape)
            .unwrap()
    );
}
