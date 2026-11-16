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
