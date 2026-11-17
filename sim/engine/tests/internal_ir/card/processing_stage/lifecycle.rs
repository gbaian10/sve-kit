use super::*;

#[test]
fn declaration_and_resolution_survive_rejected_answers() {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},"targets":[
            {"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
        ],"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        let mut game = fixture(&program, actor);
        assert_eq!(game.internal_play_phase(), None);
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            actor.player(),
        )
        .unwrap();
        let declared = question(&game, actor);
        assert!(matches!(
            declared.shape,
            QuestionShape::Decision(player::DecisionShape::Targets(_))
        ));
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Declare));
        assert_eq!(game.internal_question_stage(), None);
        let before = exact_authority(&game);
        assert_eq!(
            submit(&mut game, actor, &declared, Answer::Integer(Nat(0))),
            SubmissionKind::Rejected
        );
        assert!(
            exact_authority(&game) == before,
            "wrong shape preserves the declaration lifecycle"
        );
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Declare));
        closed_wire(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &declared, Answer::Targets(vec![0])),
            SubmissionKind::Applied
        );
        let resolving = question(&game, actor);
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        assert_eq!(game.internal_question_stage(), Some(QuestionStage::Resolve));
        assert_eq!(
            game.projection(View::Referee).unwrap()[me]["pp"]["current"],
            2
        );
        assert_eq!(
            submit(&mut game, actor, &resolving, Answer::Integer(Nat(0))),
            SubmissionKind::Applied
        );
        assert_eq!(game.internal_play_phase(), None);
    }
}

#[test]
fn a_raw_procedure_does_not_acquire_a_formal_play_lifecycle() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    let ability = &program.card().abilities[0];
    let body = WellFormedProgram::check(ability.node.body.clone().unwrap()).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = fixture(&program, actor);
        let progress = game
            .execute_internal(
                &body,
                &ability.source,
                Context {
                    source: "source".into(),
                    generation: Generation(0),
                    controller: actor.player(),
                },
            )
            .unwrap();
        assert!(
            !progress.complete,
            "the raw procedure really stopped for its optional body"
        );
        assert_eq!(game.internal_question_stage(), Some(QuestionStage::Resolve));
        assert_eq!(game.internal_play_phase(), None);
        closed_wire(&game, actor);
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(0))),
            SubmissionKind::Applied
        );
        assert_eq!(game.internal_play_phase(), None);
    }
}

pub(super) fn overflowing_program() -> WellFormedCardProgram {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},"do":[
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    let mut raw = program.card().clone();
    let changed = if let Node::Control {
        control: Control::May { body, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        && let Node::Instruction { instruction } = &mut body.steps[0].node
        && let Action::Life {
            change: NumericChange::Add { amount },
            ..
        } = &mut instruction.action
    {
        *amount = Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Add,
            arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
        }));
        true
    } else {
        false
    };
    assert!(
        changed,
        "the admitted optional body contains the intended failing life operation"
    );
    WellFormedCardProgram::check(raw).unwrap()
}

#[test]
fn an_accepted_answer_whose_body_fails_rolls_back_the_play_lifecycle() {
    let program = overflowing_program();
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
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        assert_eq!(game.internal_play_phase(), None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[me]["pp"]["current"], 3);
        assert_eq!(state[me]["leader"]["life"], 20);
    }
}
