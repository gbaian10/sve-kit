use super::*;
use core::slice::from_ref;
use sve_engine::game::internal::NumberDeclaration;

pub(in crate::card) fn with_declarations(
    program: &WellFormedCardProgram,
    actors: &[PlayerRef],
    bounded: bool,
) -> WellFormedCardProgram {
    let mut raw = program.card().clone();
    let ability = &mut raw.abilities[0];
    let scope = ability.node.body.as_ref().unwrap().scope;
    let prefix = actors
        .iter()
        .enumerate()
        .map(|(index, actor)| {
            let id = u64::try_from(index).unwrap().checked_add(1000).unwrap();
            let declaration = if bounded {
                Declaration::BoundedInteger {
                    minimum: Int(1),
                    maximum: Int(6),
                    binding: Binding::new(id, scope),
                }
            } else {
                Declaration::NonnegativeInteger {
                    binding: Binding::new(id, scope),
                }
            };
            let mut step = instruction(
                id,
                Action::Declare {
                    actor: actor.clone(),
                    declaration,
                },
            );
            step.source = ability.source.clone();
            if let Node::Instruction { instruction } = &mut step.node {
                instruction.source = ability.node.source.clone();
            }
            step
        })
        .collect::<Vec<_>>();
    ability
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .splice(0..0, prefix);
    WellFormedCardProgram::check(raw).unwrap()
}

fn declared(facts: &[Fact]) -> Vec<NumberDeclaration> {
    facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Declared { player, value } = fact {
                Some(NumberDeclaration {
                    player: *player,
                    value: *value,
                })
            } else {
                None
            }
        })
        .collect()
}

fn declaration_actor(opponent: bool) -> PlayerRef {
    if opponent {
        PlayerRef::RequireOne {
            players: Box::new(PlayerSet {
                relation: PlayerRelation::Opponents,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::One,
            }),
        }
    } else {
        PlayerRef::Controller {}
    }
}

#[test]
fn accepted_public_numbers_belong_to_their_frame_and_keep_the_actual_declarer() {
    let base = program(
        "activate",
        json!({"cost":{"pp":1},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    for controller in [Viewer::P1, Viewer::P2] {
        for opponent in [false, true] {
            let declarer = if opponent {
                if controller == Viewer::P1 {
                    Viewer::P2
                } else {
                    Viewer::P1
                }
            } else {
                controller
            };
            let wrong = if declarer == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            let actor = declaration_actor(opponent);
            for (bounded, value) in [(true, 4_u64), (false, 0), (false, u64::MAX)] {
                let program = with_declarations(&base, from_ref(&actor), bounded);
                let mut game = fixture(&program, controller);
                assert_eq!(game.internal_number_declarations(), None);
                game.activate_internal_card(
                    "source",
                    program.card().abilities[0].node.id,
                    controller.player(),
                )
                .unwrap();
                let q = question(&game, declarer);
                assert!(matches!(
                    q.shape,
                    QuestionShape::Decision(player::DecisionShape::Integer {
                        source: player::IntegerSource::NumberDeclaration,
                        ..
                    })
                ));
                assert_eq!(game.internal_number_declarations(), Some([].as_slice()));
                let before = exact_authority(&game);
                assert_eq!(
                    submit(&mut game, wrong, &q, Answer::Integer(Nat(value))),
                    SubmissionKind::Rejected
                );
                assert_eq!(
                    submit(&mut game, declarer, &q, Answer::Targets(vec![])),
                    SubmissionKind::Rejected
                );
                if bounded {
                    assert_eq!(
                        submit(&mut game, declarer, &q, Answer::Integer(Nat(0))),
                        SubmissionKind::Rejected
                    );
                }
                assert!(
                    exact_authority(&game) == before,
                    "unaccepted answers record no declaration"
                );
                assert_eq!(
                    submit(&mut game, declarer, &q, Answer::Integer(Nat(value))),
                    SubmissionKind::Applied
                );
                let expected = [NumberDeclaration {
                    player: declarer.player(),
                    value: Nat(value),
                }];
                let before = exact_authority(&game);
                assert_eq!(
                    game.internal_number_declarations().unwrap(),
                    expected.as_slice()
                );
                assert_eq!(
                    game.internal_number_declarations().unwrap(),
                    expected.as_slice()
                );
                assert!(
                    exact_authority(&game) == before,
                    "reading frame declarations is pure"
                );
                closed_wire(&game, controller);
                let progress = finish_may(&mut game);
                assert_eq!(declared(&progress.facts), expected);
                assert_eq!(game.internal_number_declarations(), None);
            }
        }
    }
}

#[test]
fn an_integer_answer_to_may_is_not_a_number_declaration() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}},
            {"optional":{"do":[{"life":{"who":"self","delta":2}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        let q = question(&game, actor);
        assert!(matches!(
            q.shape,
            QuestionShape::Decision(player::DecisionShape::May)
        ));
        assert!(
            game.internal_number_declarations().unwrap().is_empty(),
            "no declaration has executed"
        );
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::Applied
        );
        assert!(
            game.internal_number_declarations().unwrap().is_empty(),
            "Integer is an answer domain, not declaration semantics"
        );
        assert!(
            declared(&finish_may(&mut game).facts).is_empty(),
            "accepting May publishes no number declaration"
        );
        assert_eq!(game.internal_number_declarations(), None);
    }
}

#[test]
fn frame_number_declarations_are_discarded_with_a_failed_play() {
    let base = lifecycle::overflowing_program();
    let program = with_declarations(&base, &[PlayerRef::Controller {}], true);
    for actor in [Viewer::P1, Viewer::P2] {
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut game = fixture(&program, actor);
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(4))),
            SubmissionKind::Applied
        );
        assert_eq!(
            game.internal_number_declarations().unwrap(),
            &[NumberDeclaration {
                player: actor.player(),
                value: Nat(4)
            }]
        );
        let may = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &may, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_number_declarations(), None);
        assert_eq!(game.internal_play_phase(), None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[me]["pp"]["current"], 3);
        assert_eq!(state[me]["leader"]["life"], 20);
    }
}
