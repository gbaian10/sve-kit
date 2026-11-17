#[path = "play_declarations/choices.rs"]
mod choices;

use super::*;
use sve_engine::game::internal::{InputDomain, PlayDeclaration};

fn x_program() -> WellFormedCardProgram {
    program(
        "activate",
        json!({"x":{},"cost":{"pp":"x"},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower","orig_cost_le":"x"},"n":1}],
            "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    )
}

fn start(game: &mut Game, program: &WellFormedCardProgram, actor: Viewer) -> Question {
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        actor.player(),
    )
    .unwrap();
    question(game, actor)
}

fn parameters(game: &Game, expected: &[PlayDeclaration]) {
    let before = exact_authority(game);
    assert_eq!(
        game.internal_play_declarations().unwrap(),
        expected,
        "only this execution owns these declarations"
    );
    assert_eq!(
        game.internal_play_declarations().unwrap(),
        expected,
        "repeated reads retain the same parameters"
    );
    assert!(
        exact_authority(game) == before,
        "declaration reads are pure"
    );
}

fn target(game: &mut Game, actor: Viewer) {
    let native = game.internal_decision(View::Referee).unwrap();
    let domain = if let InputDomain::Targets(domain) = native.domain {
        Some(domain)
    } else {
        None
    };
    assert!(
        domain.is_some(),
        "the fixture requires one public target domain"
    );
    let domain = domain.unwrap();
    let q = question(game, actor);
    assert_eq!(
        submit(
            game,
            actor,
            &q,
            Answer::Targets(vec![domain.members.first().unwrap().handle])
        ),
        SubmissionKind::Applied,
        "the fixture's public target is accepted"
    );
}

fn decline(game: &mut Game, actor: Viewer) {
    let may = question(game, actor);
    assert!(
        matches!(
            may.shape,
            QuestionShape::Decision(player::DecisionShape::May)
        ),
        "the fixture must be at an optional resolution body"
    );
    assert_eq!(
        submit(game, actor, &may, Answer::Integer(Nat(0))),
        SubmissionKind::Applied,
        "declining the fixture body resumes its execution"
    );
}

#[test]
fn an_accepted_x_survives_targets_and_payment_without_becoming_a_free_declaration() {
    let program = x_program();
    for actor in [Viewer::P1, Viewer::P2] {
        for value in [1, 3] {
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let wrong = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            let mut game = fixture(&program, actor);
            assert_eq!(game.internal_play_declarations(), None);
            let x = start(&mut game, &program, actor);
            parameters(&game, &[]);
            let before = exact_authority(&game);
            assert_eq!(
                submit(&mut game, wrong, &x, Answer::Integer(Nat(value))),
                SubmissionKind::Rejected
            );
            assert_eq!(
                submit(&mut game, actor, &x, Answer::OptionalCost(true)),
                SubmissionKind::Rejected
            );
            assert_eq!(
                submit(&mut game, actor, &x, Answer::Integer(Nat(4))),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "rejected X answers are inert"
            );
            assert_eq!(
                submit(&mut game, actor, &x, Answer::Integer(Nat(value))),
                SubmissionKind::Applied
            );
            let expected = [PlayDeclaration::X {
                player: actor.player(),
                value: Nat(value),
            }];
            parameters(&game, &expected);
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                3
            );
            target(&mut game, actor);
            parameters(&game, &expected);
            assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
            assert_eq!(
                game.projection(View::Referee).unwrap()[me]["pp"]["current"],
                3 - value
            );
            assert!(
                game.internal_number_declarations().unwrap().is_empty(),
                "X is not a free declaration"
            );
            closed_wire(&game, actor);
            decline(&mut game, actor);
            assert_eq!(game.internal_play_declarations(), None);
        }
    }
}

#[test]
fn a_failed_body_discards_accepted_x_with_the_formal_play() {
    let program = program(
        "activate",
        json!({"x":{},"cost":{"pp":"x"},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    let program = lifecycle::with_overflow(&program);
    for actor in [Viewer::P1, Viewer::P2] {
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut game = fixture(&program, actor);
        let x = start(&mut game, &program, actor);
        assert_eq!(
            submit(&mut game, actor, &x, Answer::Integer(Nat(1))),
            SubmissionKind::Applied
        );
        parameters(
            &game,
            &[PlayDeclaration::X {
                player: actor.player(),
                value: Nat(1),
            }],
        );
        let may = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &may, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_play_declarations(), None);
        assert_eq!(game.internal_play_phase(), None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[me]["pp"]["current"], 3);
        assert_eq!(state[me]["leader"]["life"], 20);
    }
}
