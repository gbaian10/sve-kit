use super::*;

#[test]
fn resolution_declarations_keep_their_numeric_use_and_native_binding() {
    for declaration in [
        json!({"bind":"declared","kind":"number"}),
        json!({"bind":"declared","kind":"number","min":0_i32,"max":3_i32}),
    ] {
        let document = payment_fixtures::x_document(
            payment_expanded_tree(),
            "activate",
            json!({"cost":{"pp":0_i32},"do":[
                {"declare":declaration},
                {"if":{"cond":{"cmp":{"left":"declared","op":"ge","right":3_i32}},
                    "then":[{"life":{"who":"self","delta":3_i32}}]}}
            ]}),
        )
        .unwrap();
        let program = ExpandedLowerer::new()
            .unwrap()
            .lower(&document)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        for actor in [Viewer::P1, Viewer::P2] {
            let other = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            let mut game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            assert_eq!(native.purpose, player::QuestionPurpose::ResolutionChoice);
            let before = board_fixtures::exact_authority(&game);
            let (usage, shape, binding) =
                project_game(&game, actor, &QuestionId::new("number".into())).unwrap();
            assert_eq!(usage, wire::QuestionUse::DeclareNumber);
            assert_eq!(
                shape,
                wire::QuestionShape::Integer {
                    bounds: wire::Bounds {
                        min: WireNat::ZERO,
                        max: declaration.get("max").map(|_| WireNat::from(3_u32)),
                    },
                    unit: None,
                    source: None,
                }
            );
            assert_eq!(
                binding.answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Confirm { accept: true }
                ),
                Err(AnswerError)
            );
            if declaration.get("max").is_some() {
                assert_eq!(
                    binding.answer(
                        native.actor,
                        &binding.question().id,
                        &Answer::Integer {
                            value: 4_u32.into()
                        }
                    ),
                    Err(AnswerError)
                );
            }
            assert!(matches!(game.input(other), Input::Waiting(waiting)
                if waiting.actor == actor && waiting.dice.is_none()));
            game.observe(actor).unwrap();
            game.observe(other).unwrap();
            let command = binding
                .answer(
                    native.actor,
                    &binding.question().id,
                    &Answer::Integer {
                        value: 3_u32.into(),
                    },
                )
                .unwrap();
            assert_eq!(
                game.submit_player(other, command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &before);
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            let state = game.projection(View::Referee).unwrap();
            let own = if actor == Viewer::P1 { "P1" } else { "P2" };
            assert_eq!(state[own]["pp"]["current"], 3_i32);
            assert_eq!(state[own]["leader"]["life"], 23_i32);
            let after = board_fixtures::exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert_unchanged(&game, &after);
        }
    }
}
