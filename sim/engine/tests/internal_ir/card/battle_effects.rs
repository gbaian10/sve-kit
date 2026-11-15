use super::battle_pending::{board_with_program, fixture};
use super::*;
use crate::battle_costs::object;
use core::error::Error;
use sve_battle_contract::{ObjectHandle as WireHandle, observation as wire};
use sve_engine::game::player::{
    Command, DecisionShape, Input, ObjectHandle, QuestionShape, SubmissionKind, Viewer,
    observation as native,
};

fn installed(
    game: &Game,
    viewer: Viewer,
    handle: &ObjectHandle,
) -> Result<Vec<wire::InstalledEffect>, Box<dyn Error>> {
    let board = game.battle_observation(viewer, &fixture().references)?;
    let object = board
        .objects
        .get(&WireHandle::new(handle.as_str().into()))
        .ok_or("authorized fixture object")?;
    Ok(object
        .state
        .as_ref()
        .ok_or("known fixture state")?
        .effects
        .clone())
}

fn end_fixture_turn(game: &mut Game) -> Result<(), &'static str> {
    game.advance_player_input();
    for _ in 0..8 {
        // Empty decks make the first next-turn draw a terminal stop.
        if matches!(game.input(Viewer::P1), Input::Finished { .. }) {
            return Ok(());
        }
        let (viewer, question) = [Viewer::P1, Viewer::P2]
            .into_iter()
            .find_map(|viewer| {
                if let Input::Question(question) = game.input(viewer) {
                    Some((viewer, question))
                } else {
                    None
                }
            })
            .ok_or("the typed lifecycle must offer its next input")?;
        let command = if matches!(&question.shape, QuestionShape::ActionWindow(_)) {
            player_pipeline::end_command(&question)
        } else if matches!(
            &question.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        ) {
            Command::Answer {
                token: question.token,
                answer: Answer::Quick(None),
            }
        } else {
            return Err("the fixture expects only Main and Quick stops");
        };
        if game.submit_player(viewer, command).kind != SubmissionKind::Applied {
            return Err("the fixture input must advance the typed lifecycle");
        }
    }
    Err("the fixture turn did not finish")
}

pub(super) fn cap_program() -> WellFormedCardProgram {
    let payload = json!({
        "cost":{"pp":0},
        "do":[{
            "apply":{
                "on":"this",
                "effect":{"damage_cap":{"threshold":4,"set":3}},
                "until":"turn"
            }
        }]
    });
    lowerer()
        .lower(&next_damage::tree(payload))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn wire_installed_effects_project_actual_cap_installation_for_both_seats_without_secret_dependence()
{
    let program = cap_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut first = board_with_program(actor, "unit-spell", &program);
        let mut second = board_with_program(actor, "unit-amulet", &program);
        for game in [&mut first, &mut second] {
            resolved(
                game.activate_internal_card(
                    "s",
                    program.card().abilities[0].node.id,
                    actor.player(),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        let before = exact_authority(&first);
        for viewer in [Viewer::P1, Viewer::P2] {
            let native = first.observe(viewer).unwrap();
            let handle = object(&native, actor, native::ObservedZone::Field);
            let result = installed(&first, viewer, &handle).unwrap();
            let other = second.observe(viewer).unwrap();
            assert_eq!(
                result,
                installed(
                    &second,
                    viewer,
                    &object(&other, actor, native::ObservedZone::Field)
                )
                .unwrap()
            );
            assert_eq!(
                serde_json::to_value(&result).unwrap(),
                json!([{
                "source":{"kind":"card","printing_id":"printing-follower","face":1,"line":2,"holder":handle.as_str()},
                    "expires":{"kind":"turn_end"},"main_live":true,"intervals":[],"active":true,
                    "change":{"kind":"prevention","damage":"any","change":{"kind":"cap","threshold":4,"replacement":3},"remaining":null,"recipients":[{"kind":"card","object":handle.as_str()}],"sources":null}
                }])
            );
        }
        assert_eq!(exact_authority(&first), before);
        end_fixture_turn(&mut first).unwrap();
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = first.observe(viewer).unwrap();
            let handle = object(&observed, actor, native::ObservedZone::Field);
            assert_eq!(
                installed(&first, viewer, &handle).unwrap(),
                Vec::<wire::InstalledEffect>::new(),
                "the completed turn retires the actual Cap installation"
            );
        }
    }
}
