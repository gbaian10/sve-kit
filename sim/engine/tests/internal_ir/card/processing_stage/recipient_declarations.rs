use super::*;
use sve_engine::game::internal::{DamageShare, InputDomain, PlayDeclaration};

fn recipient() -> Value {
    json!({"bind":"enemy","from":{"any_of":[{"leader":"opp"},{"who":"opp","zone":"field","kind":"follower"}]},"n":1})
}

fn prepend_resolution(raw: &mut CardProgram, mut decision: Decision) {
    decision.kind = DecisionKind::ResolutionChoice {
        may: Declinable::Required,
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        precondition: None,
    };
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let mut step = body.steps[0].clone();
    step.node = Node::Decision { decision };
    body.steps.insert(0, step);
}

pub(super) fn recipient_program(later: bool) -> WellFormedCardProgram {
    let mut targets = vec![recipient()];
    if later {
        let mut second = recipient();
        second["bind"] = json!("later");
        targets.push(second);
    }
    let base = program(
        "activate",
        json!({"cost":{"pp":1},"targets":targets,
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    if !later {
        return base;
    }
    let mut raw = base.card().clone();
    // Lowering allocates the second decision's selector scopes independently of the first.
    let choice = raw.abilities[0]
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .pop()
        .unwrap();
    prepend_resolution(&mut raw, choice);
    WellFormedCardProgram::check(raw).unwrap()
}

pub(super) fn distribution_program(with_recipient: bool) -> WellFormedCardProgram {
    let mut targets = vec![];
    if with_recipient {
        targets.push(recipient());
    }
    targets.push(
        json!({"bind":"objects","from":{"who":"opp","zone":"field","kind":"follower"},"upto":4}),
    );
    let mut body = json!({"cost":{"pp":1},"targets":targets,
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]});
    if with_recipient {
        body["x"] = json!({});
        body["cost"] = json!({"pp":"x"});
    }
    append_distribution(&program("activate", body))
}

pub(super) fn append_distribution(program: &WellFormedCardProgram) -> WellFormedCardProgram {
    let mut raw = program.card().clone();
    let parameters = &mut raw.abilities[0].node.play.as_mut().unwrap().parameters;
    let target = parameters.iter().rev().find(|decision| {
        matches!(
            decision.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::Targets,
                ..
            }
        ) && matches!(decision.domain, ChoiceDomain::Objects { .. })
    });
    assert!(
        target.is_some(),
        "the fixture has a formal object target before distribution"
    );
    let mut allocation = target.unwrap().clone();
    let reference = if let ChoiceDomain::Objects { binding, .. } = &allocation.domain {
        Some(*binding)
    } else {
        None
    };
    assert!(
        reference.is_some(),
        "the fixture's final original parameter selects objects"
    );
    allocation.id = DecisionId(1000);
    allocation.captures.clear();
    allocation.domain = ChoiceDomain::Distribution {
        recipients: RecipientSet::Objects {
            objects: Selector::Bound {
                reference: reference.unwrap(),
            },
        },
        total: Expr::Literal(Nat(3)),
        minimum: Nat(1),
        binding: Binding::new(1000, ScopeId(1)),
    };
    let changed = if let DecisionKind::PlayParameter { slot, .. } = &mut allocation.kind {
        *slot = PlaySlot::Distribution;
        true
    } else {
        false
    };
    assert!(changed, "the allocation is a formal play parameter");
    parameters.push(allocation);
    WellFormedCardProgram::check(raw).unwrap()
}

pub(super) fn start(program: &WellFormedCardProgram, actor: Viewer, fields: usize) -> Game {
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut game = fixture_with(program, actor, |initial| {
        initial["players"][them]["zones"]["field"] = json!(
            (0..fields)
                .map(|i| json!({"id":format!("f{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    });
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        actor.player(),
    )
    .unwrap();
    game
}

fn recipient_handle(game: &Game, leader: bool) -> u64 {
    let native = game.internal_decision(View::Referee).unwrap();
    let domain = if let InputDomain::Targets(domain) = native.domain {
        Some(domain)
    } else {
        None
    };
    assert!(domain.is_some(), "the fixture must offer recipient targets");
    domain
        .unwrap()
        .members
        .iter()
        .find(|member| member.card.is_none() == leader)
        .unwrap()
        .handle
}

fn apply(game: &mut Game, actor: Viewer, answer: Answer) {
    let q = question(game, actor);
    assert_eq!(
        submit(game, actor, &q, answer),
        SubmissionKind::Applied,
        "the fixture submits a legal answer to its own current question"
    );
}

fn declarations(game: &Game) -> Vec<PlayDeclaration> {
    let before = exact_authority(game);
    let result = game.internal_play_declarations().unwrap().to_vec();
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_play_declarations().unwrap(),
        result.as_slice(),
        "observing either seat does not declare anything"
    );
    assert!(
        exact_authority(game) == before,
        "declaration reads are pure"
    );
    result
}

pub(super) fn shares() -> Answer {
    Answer::Distribution(vec![
        DamageShare {
            handle: 1,
            amount: Nat(1),
        },
        DamageShare {
            handle: 0,
            amount: Nat(2),
        },
    ])
}

#[test]
fn a_formal_recipient_target_preserves_the_accepted_leader() {
    let program = recipient_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        for leader in [false, true] {
            let mut game = start(&program, actor, 2);
            assert!(
                declarations(&game).is_empty(),
                "a question has no accepted declaration yet"
            );
            let native = game.internal_decision(View::Referee).unwrap();
            let handle = recipient_handle(&game, leader);
            let q = question(&game, actor);
            let before = exact_authority(&game);
            assert_eq!(
                submit(&mut game, actor, &q, Answer::Targets(vec![99])),
                SubmissionKind::Rejected
            );
            assert_eq!(
                submit(&mut game, actor, &q, Answer::Targets(vec![handle, handle])),
                SubmissionKind::Rejected
            );
            let wrong = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            assert_eq!(
                submit(&mut game, wrong, &q, Answer::Targets(vec![handle])),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "rejected recipients cannot leave declarations"
            );
            apply(&mut game, actor, Answer::Targets(vec![handle]));
            let saved = declarations(&game);
            assert_eq!(saved.len(), 1);
            let PlayDeclaration::Recipients {
                player,
                decision,
                members,
            } = &saved[0]
            else {
                panic!("formal recipients")
            };
            assert_eq!(*player, actor.player());
            assert_eq!(*decision, native.token.decision);
            assert_eq!(members.len(), 1);
            if leader {
                assert_eq!(
                    members[0],
                    RuntimeDamageRecipient::Leader {
                        player: wrong.player()
                    }
                );
            } else {
                assert!(matches!(members[0], RuntimeDamageRecipient::Object { .. }));
            }
            assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
            closed_wire(&game, actor);
            finish_may(&mut game);
            assert_eq!(game.internal_play_declarations(), None);
        }
    }
}

#[test]
fn a_resolution_recipient_choice_does_not_append_a_formal_declaration() {
    let program = recipient_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        let leader = recipient_handle(&game, true);
        apply(&mut game, actor, Answer::Targets(vec![leader]));
        let formal = declarations(&game);
        assert_eq!(formal.len(), 1);
        assert_eq!(
            question(&game, actor).purpose,
            QuestionPurpose::ResolutionChoice
        );
        let follower = recipient_handle(&game, false);
        apply(&mut game, actor, Answer::Targets(vec![follower]));
        assert_eq!(declarations(&game), formal);
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn a_formal_distribution_keeps_resolved_members_and_declared_amounts_in_answer_order() {
    let program = distribution_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        apply(&mut game, actor, Answer::Targets(vec![1, 0]));
        let targets = game.internal_target_declarations().unwrap()[0]
            .objects
            .clone();
        assert_eq!(targets.len(), 2);
        assert!(
            declarations(&game).is_empty(),
            "distribution has not been answered"
        );
        let native = game.internal_decision(View::Referee).unwrap();
        let q = question(&game, actor);
        let before = exact_authority(&game);
        assert_eq!(
            submit(
                &mut game,
                actor,
                &q,
                Answer::Distribution(vec![DamageShare {
                    handle: 0,
                    amount: Nat(3)
                }])
            ),
            SubmissionKind::Rejected
        );
        assert_eq!(
            submit(
                &mut game,
                actor,
                &q,
                Answer::Distribution(vec![
                    DamageShare {
                        handle: 0,
                        amount: Nat(1)
                    },
                    DamageShare {
                        handle: 1,
                        amount: Nat(1)
                    }
                ])
            ),
            SubmissionKind::Rejected
        );
        assert!(
            exact_authority(&game) == before,
            "incomplete and wrong-total shares are inert"
        );
        apply(&mut game, actor, shares());
        let saved = declarations(&game);
        assert_eq!(
            saved,
            vec![PlayDeclaration::Distribution {
                player: actor.player(),
                decision: native.token.decision,
                allocations: vec![
                    ResolvedDamageAllocation {
                        recipient: RuntimeDamageRecipient::Object { object: targets[1] },
                        amount: Nat(1)
                    },
                    ResolvedDamageAllocation {
                        recipient: RuntimeDamageRecipient::Object { object: targets[0] },
                        amount: Nat(2)
                    },
                ],
            }]
        );
        closed_wire(&game, actor);
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn an_automatic_empty_distribution_is_not_a_player_answer() {
    let program = distribution_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        apply(&mut game, actor, Answer::Targets(vec![]));
        assert!(
            declarations(&game).is_empty(),
            "the empty distribution was never answered"
        );
        assert_eq!(
            question(&game, actor).purpose,
            QuestionPurpose::ResolutionChoice
        );
        assert_eq!(game.internal_target_declarations().unwrap().len(), 1);
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn a_retry_discards_earlier_recipients_but_keeps_the_accepted_x() {
    let program = distribution_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 4);
        apply(&mut game, actor, Answer::Integer(Nat(1)));
        let x = vec![PlayDeclaration::X {
            player: actor.player(),
            value: Nat(1),
        }];
        assert_eq!(declarations(&game), x);
        let leader = recipient_handle(&game, true);
        apply(&mut game, actor, Answer::Targets(vec![leader]));
        assert_eq!(declarations(&game).len(), 2);
        apply(&mut game, actor, Answer::Targets(vec![0, 1, 2, 3]));
        assert_eq!(declarations(&game), x);
        assert!(
            game.internal_target_declarations().unwrap().is_empty(),
            "the entire failed selection round was discarded"
        );
        let follower = recipient_handle(&game, false);
        apply(&mut game, actor, Answer::Targets(vec![follower]));
        let second = declarations(&game);
        assert!(
            matches!(&second[1], PlayDeclaration::Recipients { members, .. } if matches!(members.as_slice(), [RuntimeDamageRecipient::Object { .. }]))
        );
        apply(&mut game, actor, Answer::Targets(vec![0, 1]));
        apply(&mut game, actor, shares());
        let complete = declarations(&game);
        assert_eq!(complete.len(), 3);
        assert_eq!(complete[..2], second);
        assert!(matches!(complete[2], PlayDeclaration::Distribution { .. }));
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn a_later_body_fault_discards_recipient_and_allocation_declarations() {
    for distribution in [false, true] {
        let base = if distribution {
            distribution_program(false)
        } else {
            recipient_program(false)
        };
        let program = lifecycle::with_overflow(&base);
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = start(&program, actor, 2);
            if distribution {
                apply(&mut game, actor, Answer::Targets(vec![0, 1]));
                apply(&mut game, actor, shares());
            } else {
                let leader = recipient_handle(&game, true);
                apply(&mut game, actor, Answer::Targets(vec![leader]));
            }
            assert_eq!(declarations(&game).len(), 1);
            let may = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &may, Answer::Integer(Nat(1))),
                SubmissionKind::EngineFault
            );
            assert_eq!(game.internal_play_declarations(), None);
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                3
            );
        }
    }
}

#[test]
fn a_resolution_distribution_is_not_recorded_as_a_formal_parameter() {
    let program = checked(vec![
        allocation::distribution(1001, allocation::leaders(), Expr::Literal(Nat(3)), 0),
        allocation::distribution(1002, allocation::leaders(), Expr::Literal(Nat(3)), 0),
    ]);
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let progress = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert!(!progress.complete);
        assert!(
            declarations(&game).is_empty(),
            "a resolution question is not a formal declaration"
        );
        let first = game.internal_decision(View::Referee).unwrap();
        let progress = game
            .answer_internal(first.actor, first.token, shares())
            .unwrap();
        assert!(!progress.complete);
        assert_eq!(
            game.internal_decision(View::Referee)
                .unwrap()
                .token
                .decision,
            DecisionId(1002)
        );
        assert!(
            declarations(&game).is_empty(),
            "accepting resolution shares does not record a formal parameter"
        );
        let second = game.internal_decision(View::Referee).unwrap();
        let progress = game
            .answer_internal(second.actor, second.token, shares())
            .unwrap();
        assert!(progress.complete);
        assert_eq!(game.internal_play_declarations(), None);
    }
}
