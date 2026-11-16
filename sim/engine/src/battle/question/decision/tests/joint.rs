use super::super::*;
use super::select::{board, components, domain, program, project_question, question, seats};
use crate::battle::tests::board_fixtures;
use crate::game::player::{Question, SubmissionKind, Viewer};
use crate::game::{Game, View};
use serde_json::json;
use sve_battle_contract::{Nat as WireNat, QuestionId, Share};

fn distribution(actor: Viewer) -> Game {
    let program = program(json!({
        "cost":{"pp":1_i32},
        "targets":[{"bind":"enemies","from":{"who":"opp","zone":"field","kind":"follower"},"any":true}],
        "do":[{"damage":{"to":"enemies","n":2_i32,"distribute":true}}]
    }));
    let mut game = board(actor, &program);
    let native = question(&game, actor).unwrap();
    let DecisionShape::Targets(selection) = domain(&native).unwrap() else {
        panic!("formal targets precede their allocation");
    };
    let handles = selection
        .members
        .iter()
        .map(|member| member.handle)
        .collect();
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: native.token,
                answer: player::Answer::Targets(handles)
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    assert!(
        matches!(
            domain(&question(&game, actor).unwrap()),
            Some(DecisionShape::Distribution(_))
        ),
        "formal targets produce an allocation stop"
    );
    game
}

fn projected(
    game: &Game,
    actor: Viewer,
) -> (Question, wire::QuestionUse, wire::QuestionShape, Binding) {
    let native = question(game, actor).unwrap();
    let (usage, shape, binding) = project_question(
        &native,
        &components(game, actor),
        &QuestionId::new("joint".into()),
    )
    .unwrap();
    (native, usage, shape, binding)
}

#[test]
fn distribution_preserves_frozen_recipients_and_native_damage_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = distribution(actor);
        let before = board_fixtures::exact_authority(&game);
        let (native, usage, shape, binding) = projected(&game, actor);
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "projection preserves authority"
        );
        assert_eq!(usage, wire::QuestionUse::DamageDistribution);
        let wire::QuestionShape::Distribute {
            recipients,
            total,
            share,
            empty,
        } = shape
        else {
            panic!("an allocation retains its answer shape");
        };
        assert_eq!(total, WireNat::from(2_u32));
        assert_eq!(share.min, WireNat::from(1_u32));
        assert_eq!(share.max, Some(total));
        assert_eq!(empty, wire::EmptyDistribution::AllowWithoutTotal);
        assert_eq!(recipients.len(), 1);
        let answer = Answer::Distribute {
            shares: vec![Share {
                option: recipients.first().unwrap().id.clone(),
                amount: total,
            }],
        };
        let command = binding.answer(native.token, &answer).unwrap();
        assert!(matches!(
            &command,
            player::Command::Answer {
                answer: player::Answer::Distribution(_),
                ..
            }
        ));
        submit_exact(&mut game, &native, &binding, &answer);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_i32
        );
    }
}

mod resource;

mod drive;

fn submit_exact(game: &mut Game, native: &Question, binding: &Binding, answer: &Answer) {
    let other = if native.actor == Viewer::P1 {
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
    assert!(
        board_fixtures::exact_authority(game) == before,
        "wrong actor preserves the complete authority"
    );
    assert_eq!(
        game.submit_player(native.actor, command.clone()).kind,
        SubmissionKind::Applied
    );
    let after = board_fixtures::exact_authority(game);
    assert_eq!(
        game.submit_player(native.actor, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        board_fixtures::exact_authority(game) == after,
        "stale answer preserves the complete authority"
    );
}

mod allocation;
