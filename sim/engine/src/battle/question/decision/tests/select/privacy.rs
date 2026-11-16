use super::*;

pub(super) fn cases() -> [(Value, wire::QuestionUse, usize); 4] {
    [
        (
            json!({"cost":{"pp":1_i32},"do":[
                {"choose":{"bind":"picked","from":{"who":"self","zone":"hand","kind":"follower"},"upto":1_i32}},
                {"move":{"ref":"picked","to":"field","result":"entered"}},
                {"life":{"who":"self","delta":1_i32}}
            ]}),
            wire::QuestionUse::ResolutionSelection,
            1,
        ),
        (
            json!({"cost":{"steps":[{"pp":1_i32},{"reveal":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1_i32}}]},
            "do":[{"life":{"who":"self","delta":1_i32}}]}),
            wire::QuestionUse::CostMaterial,
            1,
        ),
        (
            json!({"cost":{"steps":[{"pp":1_i32},{"discard":{"groups":[
            {"sel":{"who":"self","zone":"hand","kind":"follower"},"n":1_i32},
            {"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1_i32}
        ]}}]},"do":[{"life":{"who":"self","delta":1_i32}}]}),
            wire::QuestionUse::CostDiscard,
            2,
        ),
        (
            json!({"cost":{"pp":1_i32},"do":[{"discard":{"who":"self","n":1_i32}},
            {"life":{"who":"self","delta":1_i32}}]}),
            wire::QuestionUse::EffectDiscard,
            1,
        ),
    ]
}

fn assert_unchanged(game: &Game, before: &str) {
    assert!(
        board_fixtures::exact_authority(game) == before,
        "selection reads and rejected submissions preserve authority"
    );
}

fn submit_complete(game: &mut Game, native: &Question, binding: &Binding, answer: &Answer) {
    let actor = native.actor;
    let other = if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    };
    let before = board_fixtures::exact_authority(game);
    let command = binding.answer(native.token, answer).unwrap();
    assert_eq!(
        game.submit_player(other, command.clone()).kind,
        SubmissionKind::Rejected
    );
    assert_unchanged(game, &before);
    assert_eq!(
        game.submit_player(actor, command.clone()).kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state[seats(actor).0]["pp"]["current"], 2_i32);
    assert_eq!(state[seats(actor).0]["leader"]["life"], 21_i32);
    let after = board_fixtures::exact_authority(game);
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Rejected
    );
    assert_unchanged(game, &after);
}

fn waiting_view_is_private(
    program: &WellFormedCardProgram,
    actor: Viewer,
    expected: wire::QuestionUse,
    count: usize,
    id: &QuestionId,
) {
    let references = &board_fixture::fixture().unwrap().references;
    let other = if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    };
    let mut waiting = None;
    let mut waiting_board = None;
    let mut waiting_components = None;
    let mut candidates = Vec::new();
    let mut native_candidates = Vec::new();
    let hands = [
        vec!["unit-follower", "unit-spell", "unit-amulet"],
        if expected == wire::QuestionUse::ResolutionSelection {
            vec!["unit-follower", "unit-follower", "unit-spell"]
        } else {
            vec!["unit-follower", "unit-spell", "unit-spell"]
        },
    ];
    for own in hands {
        let mut game = board_with(actor, program, &own, &["unit-spell"]);
        let before = board_fixtures::exact_authority(&game);
        let native = question(&game, actor).unwrap();
        native_candidates.push(native_selection(&native).unwrap().members.clone());
        let observation = components(&game, actor);
        let (usage, shape, binding) = project_question(&native, &observation, id).unwrap();
        assert_eq!(usage, expected);
        let offered = selection(&shape).unwrap();
        candidates.push(offered.clone());
        let summary = game.input(other);
        assert!(matches!(summary, Input::Waiting(_)));
        let hidden_board = game.observe(other).unwrap();
        let hidden_components = components(&game, other);
        if let Some(previous) = &waiting {
            assert!(
                previous == &summary,
                "candidate differences cannot change the complete Waiting summary"
            );
            assert!(
                waiting_board.as_ref() == Some(&hidden_board),
                "candidate differences cannot change the waiting seat's complete native board"
            );
            assert!(
                waiting_components.as_ref() == Some(&hidden_components),
                "candidate differences cannot change waiting board components"
            );
        }
        waiting = Some(summary);
        waiting_board = Some(hidden_board);
        waiting_components = Some(hidden_components);
        assert!(
            game.battle_question(other, id, references)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            game.battle_question(actor, id, references),
            Err(ProjectionError::Unavailable)
        ));
        assert_unchanged(&game, &before);
        let answer = Answer::Select {
            selected: offered
                .candidates
                .iter()
                .take(count)
                .map(|candidate| candidate.id.clone())
                .collect(),
            groups: None,
            roles: vec![],
        };
        submit_complete(&mut game, &native, &binding, &answer);
    }
    assert!(
        candidates.first() != candidates.last(),
        "the two worlds must really offer different candidate identities or counts"
    );
    assert!(
        native_candidates.first() != native_candidates.last(),
        "native candidates must genuinely differ before testing projection privacy"
    );
}

fn opponent_secrets_preserve_actor_view(
    program: &WellFormedCardProgram,
    actor: Viewer,
    id: &QuestionId,
) {
    let mut actor_board = None;
    let mut actor_shape = None;
    let mut actor_components = None;
    for secret in ["unit-spell", "unit-amulet"] {
        let game = board_with(
            actor,
            program,
            &["unit-follower", "unit-spell", "unit-amulet"],
            &[secret],
        );
        let native = question(&game, actor).unwrap();
        let observation = components(&game, actor);
        let (_, shape, _) = project_question(&native, &observation, id).unwrap();
        let public = game.observe(actor).unwrap();
        if let Some(previous) = &actor_board {
            assert!(
                previous == &public,
                "opponent secrets cannot change the actor's complete native board"
            );
            assert!(
                actor_shape.as_ref() == Some(&shape),
                "opponent secrets cannot change the actor's complete selection shape"
            );
            assert!(
                actor_components.as_ref() == Some(&observation),
                "opponent secrets cannot change the actor's complete board components"
            );
        }
        actor_board = Some(public);
        actor_shape = Some(shape);
        actor_components = Some(observation);
    }
}

#[test]
fn hidden_selection_sources_preserve_both_seats_complete_views_before_payment() {
    let id = QuestionId::new("private".into());
    for (body, expected, count) in cases() {
        let program = program(body);
        for actor in [Viewer::P1, Viewer::P2] {
            waiting_view_is_private(&program, actor, expected, count, &id);
            opponent_secrets_preserve_actor_view(&program, actor, &id);
        }
    }
}

#[test]
fn an_empty_private_resolution_selection_is_a_real_zero_candidate_stop() {
    let (body, _, _) = cases().into_iter().next().unwrap();
    let program = program(body);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_with(
            actor,
            &program,
            &["unit-spell", "unit-amulet"],
            &["unit-spell"],
        );
        let native = question(&game, actor).unwrap();
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("empty".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::ResolutionSelection);
        assert_eq!(selection(&shape).unwrap().candidates, []);
        let command = binding.answer(native.token, &pick_first(&shape)).unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            21_i32
        );
    }
}
