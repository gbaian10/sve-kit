use super::super::*;
use super::select::{board, components, domain, project_question, question, seats};
use crate::battle::tests::{board_fixtures, payment_fixtures};
use crate::game::View;
use crate::game::player::{SubmissionKind, Viewer};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{WellFormedCardProgram, payment_expanded_tree};
use serde_json::json;
use sve_battle_contract::{QuestionId, SourceRef};

fn program() -> WellFormedCardProgram {
    let mut document = payment_fixtures::x_document(
        payment_expanded_tree(),
        "activate",
        json!({
            "cost":{"pp":0_i32},"do":[{"life":{"who":"self","delta":1_i32}},
            {"play_ability":{"of":"this","trigger":"evolve_time"}},
            {"life":{"who":"self","delta":10_i32}}]
        }),
    )
    .unwrap();
    let mut child = document["cards"]["unit-follower"]["abilities"][0].clone();
    child.as_object_mut().unwrap().remove("activate");
    child["line"] = json!(4_i32);
    child["evolve_time"] =
        json!({"cost":{"pp":0_i32},"do":[{"life":{"who":"self","delta":2_i32}}]});
    document["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(child);
    ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn effective_ability_choices_keep_the_authoritative_source_and_native_handle() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program());
        let native = question(&game, actor).unwrap();
        assert!(
            matches!(domain(&native),Some(DecisionShape::Abilities(options)) if options.len()==1)
        );
        let before = board_fixtures::exact_authority(&game);
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("ability".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::Ability);
        let options = if let wire::QuestionShape::Choose {
            options,
            bounds,
            count,
            ..
        } = shape
        {
            assert_eq!((bounds.min.get(), bounds.max.unwrap().get()), (1, 1));
            assert!(count.is_none());
            Some(options)
        } else {
            None
        }
        .unwrap();
        assert!(
            matches!(&options.first().unwrap().value, wire::ChoiceValue::Ability { source: SourceRef::Card { printing_id,face:Some(face),line:Some(line),holder:Some(_) } }
            if printing_id=="printing-follower" && face.get()==1 && line.get()==4)
        );
        let answer = Answer::Choose {
            chosen: vec![options.first().unwrap().id.clone()],
        };
        assert_eq!(binding.decode(&answer), Ok(player::Answer::Ability(0)));
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "ability projection preserves authority"
        );
        let command = binding.answer(native.token, &answer).unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let payment = question(&game, actor).unwrap();
        assert_eq!(domain(&payment), Some(&DecisionShape::OptionalCost));
        assert_eq!(
            game.submit_player(
                actor,
                player::Command::Answer {
                    token: payment.token,
                    answer: player::Answer::OptionalCost(true)
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            33_i32
        );
    }
}

#[test]
fn ability_rejects_invalid_coordinates_wrong_viewer_and_stale_answers_without_mutation() {
    use sve_battle_contract::OptionId;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut game = board(actor, &program());
        let native = question(&game, actor).unwrap();
        let id = QuestionId::new("invalid-ability".into());
        let observation = components(&game, actor);
        let (_, shape, binding) = project_question(&native, &observation, &id).unwrap();
        let wire::QuestionShape::Choose { options, .. } = shape else {
            panic!("ability choose")
        };
        for answer in [
            Answer::Confirm { accept: true },
            Answer::Choose { chosen: vec![] },
            Answer::Choose {
                chosen: vec![options[0].id.clone(); 2],
            },
            Answer::Choose {
                chosen: vec![OptionId::new("foreign".into())],
            },
        ] {
            assert_eq!(binding.decode(&answer), Err(AnswerError));
        }
        let before = board_fixtures::exact_authority(&game);
        let answer = Answer::Choose {
            chosen: vec![options[0].id.clone()],
        };
        assert!(project_question(&native, &components(&game, other), &id).is_err());
        assert_eq!(
            game.submit_player(other, binding.answer(native.token, &answer).unwrap())
                .kind,
            SubmissionKind::Rejected
        );
        assert_eq!(
            game.submit_player(
                actor,
                player::Command::Answer {
                    token: native.token,
                    answer: player::Answer::Ability(999)
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "projection and rejected answers preserve authority"
        );
        assert_eq!(
            game.submit_player(actor, binding.answer(native.token, &answer).unwrap())
                .kind,
            SubmissionKind::Applied
        );
        let current = board_fixtures::exact_authority(&game);
        assert_eq!(
            game.submit_player(actor, binding.answer(native.token, &answer).unwrap())
                .kind,
            SubmissionKind::Rejected
        );
        assert!(
            board_fixtures::exact_authority(&game) == current,
            "projection and rejected answers preserve authority"
        );
    }
}

#[test]
fn no_effective_child_skips_the_native_ability_question() {
    let mut card = program().card().clone();
    card.abilities.truncate(1);
    let program = WellFormedCardProgram::check(card).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let game = board(actor, &program);
        assert!(!matches!(
            question(&game, actor).as_ref().and_then(domain),
            Some(DecisionShape::Abilities(_))
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            31_i32
        );
    }
}

#[test]
fn ability_sources_require_captured_evidence_and_current_capabilities() {
    for actor in [Viewer::P1, Viewer::P2] {
        let game = board(actor, &program());
        let native = question(&game, actor).unwrap();
        let DecisionShape::Abilities(choices) = domain(&native).unwrap() else {
            panic!("abilities")
        };
        let id = QuestionId::new("ability-evidence".into());
        let observation = components(&game, actor);
        let before = board_fixtures::exact_authority(&game);
        for mode in 0_u8..3 {
            let mut invalid = native.clone();
            let player::QuestionShape::Decision(DecisionShape::Abilities(options)) =
                &mut invalid.shape
            else {
                panic!("abilities")
            };
            match mode {
                0 => options[0].source = None,
                1 => options.push(choices[0].clone()),
                _ => {
                    options[0].source.as_mut().unwrap().holder =
                        Some(player::ObjectHandle("expired-capability".into()));
                }
            }
            assert!(project_question(&invalid, &observation, &id).is_err());
        }
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "projection and rejected answers preserve authority"
        );
    }
}

mod privacy;

mod current;
