#[path = "processing_stage/recipient_declarations.rs"]
mod recipient_declarations;

#[path = "processing_stage/target_declarations.rs"]
mod target_declarations;

#[path = "processing_stage/cost_spec_payments.rs"]
mod cost_spec_payments;

#[path = "processing_stage/pp_payments.rs"]
mod pp_payments;

#[path = "processing_stage/play_declarations.rs"]
mod play_declarations;

#[path = "processing_stage/declarations.rs"]
pub(super) mod declarations;

#[path = "processing_stage/lifecycle.rs"]
mod lifecycle;

use super::*;
use crate::player_pipeline::question;
use crate::{board_fixture, payment_fixtures};
use sve_battle_contract::QuestionId;
use sve_engine::battle::ProjectionError;
use sve_engine::game::internal::{PlayPhase, QuestionStage};
use sve_engine::game::player::{
    self, Command, Question, QuestionPurpose, QuestionShape, SubmissionKind, Viewer,
};

fn program(head: &str, body: Value) -> WellFormedCardProgram {
    let document = payment_fixtures::x_document(fanfare::expanded::tree(), head, body).unwrap();
    lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn fixture(program: &WellFormedCardProgram, actor: Viewer) -> Game {
    fixture_with(program, actor, |_| {})
}

fn fixture_with(
    program: &WellFormedCardProgram,
    actor: Viewer,
    customize: impl FnOnce(&mut Value),
) -> Game {
    let (seat, me, them) = if actor == Viewer::P1 {
        (1, "P1", "P2")
    } else {
        (2, "P2", "P1")
    };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["pp"] = json!({"current":3,"max":3});
    initial["players"][me]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}]});
    initial["players"][them]["zones"] = json!({"field":[
        {"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"}
    ]});
    customize(&mut initial);
    let fixture = board_fixture::fixture().unwrap();
    let mut game = Game::new(
        Arc::clone(&fixture.catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "question-stage",
    )
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.load_internal_card("unit-follower", program).unwrap();
    game
}

fn submit(game: &mut Game, actor: Viewer, q: &Question, answer: Answer) -> SubmissionKind {
    game.submit_player(
        actor,
        Command::Answer {
            token: q.token,
            answer,
        },
    )
    .kind
}

fn closed_wire(game: &Game, actor: Viewer) {
    let fixture = board_fixture::fixture().unwrap();
    assert!(
        matches!(
            game.battle_question(actor, &QuestionId::new("stage".into()), &fixture.references),
            Err(ProjectionError::Unavailable)
        ),
        "a question phase alone does not authorize wire interaction"
    );
}

fn end_trigger(game: &mut Game, actor: Viewer) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game.advance_player_input();
    let pending = question(game, actor);
    let choices =
        if let QuestionShape::Decision(player::DecisionShape::Pending(choices)) = &pending.shape {
            Some(choices)
        } else {
            None
        };
    assert!(
        choices.is_some(),
        "the fixture requires a pending trigger question"
    );
    let choices = choices.unwrap();
    assert_eq!(
        choices.len(),
        1,
        "the fixture installs one eligible end trigger"
    );
    assert_eq!(
        submit(game, actor, &pending, Answer::Pending(choices[0].handle)),
        SubmissionKind::Applied,
        "choosing the pending trigger enters its play"
    );
}

fn finish_may(game: &mut Game) -> Progress {
    let q = game.internal_decision(View::Referee).unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(0)))
            .unwrap(),
    )
    .progress
}

#[test]
fn may_stage_is_recorded_and_not_inherited_by_an_unrecorded_target_stop() {
    let program = program(
        "activate",
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[
            {"choose":{"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}}
        ]}}]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let seat = if actor == Viewer::P1 { 1 } else { 2 };
        for accept in [false, true] {
            let mut game = fixture(&program, actor);
            assert_eq!(game.internal_question_stage(), None);
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat },
            )
            .unwrap();
            let q = question(&game, actor);
            assert!(matches!(
                q.shape,
                QuestionShape::Decision(player::DecisionShape::May)
            ));
            assert_eq!(q.purpose, QuestionPurpose::ResolutionChoice);
            assert_eq!(game.internal_question_stage(), Some(QuestionStage::Resolve));
            let before = exact_authority(&game);
            closed_wire(&game, actor);
            assert_eq!(
                submit(&mut game, other, &q, Answer::Integer(Nat(0))),
                SubmissionKind::Rejected
            );
            assert_eq!(
                submit(&mut game, actor, &q, Answer::OptionalCost(false)),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "rejected answers preserve authority"
            );
            assert_eq!(game.internal_question_stage(), Some(QuestionStage::Resolve));
            assert_eq!(
                submit(
                    &mut game,
                    actor,
                    &q,
                    Answer::Integer(Nat(u64::from(accept)))
                ),
                SubmissionKind::Applied
            );
            assert_eq!(game.internal_question_stage(), None);
            if accept {
                let next = question(&game, actor);
                assert!(matches!(
                    next.shape,
                    QuestionShape::Decision(player::DecisionShape::Targets(_))
                ));
                assert_eq!(next.purpose, QuestionPurpose::ResolutionChoice);
                closed_wire(&game, actor);
            }
        }
    }
}

#[test]
fn resolution_payment_has_a_distinct_stage_with_the_same_native_purpose() {
    let program = program(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[
            {"pay":{"cost":{"pp":1},"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let me = if actor == Viewer::P1 { "P1" } else { "P2" };
        for accept in [false, true] {
            let mut game = fixture(&program, actor);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            game.advance_player_input();
            let pending = question(&game, actor);
            let QuestionShape::Decision(player::DecisionShape::Pending(choices)) = &pending.shape
            else {
                panic!("one native end trigger");
            };
            assert_eq!(choices.len(), 1);
            assert_eq!(
                submit(
                    &mut game,
                    actor,
                    &pending,
                    Answer::Pending(choices[0].handle)
                ),
                SubmissionKind::Applied
            );
            let q = question(&game, actor);
            assert!(matches!(
                q.shape,
                QuestionShape::Decision(player::DecisionShape::ResolutionPayment)
            ));
            assert_eq!(q.purpose, QuestionPurpose::ResolutionChoice);
            assert_eq!(game.internal_question_stage(), Some(QuestionStage::Pay));
            assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
            let before = exact_authority(&game);
            closed_wire(&game, actor);
            assert_eq!(
                submit(&mut game, other, &q, Answer::OptionalCost(accept)),
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "wrong-seat payment preserves authority"
            );
            assert_eq!(game.internal_question_stage(), Some(QuestionStage::Pay));
            assert_eq!(game.internal_play_phase(), Some(PlayPhase::Resolve));
            assert_eq!(
                submit(&mut game, actor, &q, Answer::OptionalCost(accept)),
                SubmissionKind::Applied
            );
            assert_eq!(game.internal_question_stage(), None);
            assert_eq!(game.internal_play_phase(), None);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[me]["pp"]["current"], if accept { 2 } else { 3 });
            assert_eq!(state[me]["leader"]["life"], if accept { 21 } else { 20 });
        }
    }
}
