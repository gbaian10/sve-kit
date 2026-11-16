use super::*;
use crate::battle::tests::payment_fixtures;
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{CardProgram, TriggerIcon, WellFormedCardProgram, payment_expanded_tree};
use serde_json::Value;

fn make(actor: Viewer, icon: &str) -> Game {
    let document = payment_fixtures::x_document(
        payment_expanded_tree(),
        "ride",
        json!({"cost":{"pp":1_u32}}),
    )
    .unwrap();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["construction"] = json!("title");
    initial["players"][own]["title_code"] = json!("synthetic");
    initial["players"][own]["leader"]["life"] = json!(10_u32);
    initial["players"][own]["zones"] = json!({
        "field":[{"id":"source","card":"unit-follower"}],
        "evolve_deck":[{"id":"drive-point","card":"unit-spell"}],
        "deck":[{"id":"checked","card":"unit-spell"},{"id":"drawn","card":"unit-amulet"}]
    });
    initial["players"][other]["zones"] = json!({});
    let facts = json!({"unit-spell":{"verified":true,"trigger_icon":icon}});
    let mut game = Game::new(
        resource::catalog(),
        &initial,
        &facts,
        &Value::Null,
        "decision-drive",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("unit-spell", &empty).unwrap();
    game.load_internal_card("unit-amulet", &empty).unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        actor.player(),
    )
    .unwrap();
    game.advance_player_input();
    let payment = question(&game, actor).unwrap();
    let Some(DecisionShape::ResourceEntryPayment { materials, .. }) = domain(&payment) else {
        panic!("Ride uses the native resource entry payment");
    };
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: payment.token,
                answer: player::Answer::ResourceEntryPayment {
                    materials: materials
                        .members
                        .iter()
                        .map(|member| member.handle)
                        .collect(),
                    use_ep: false
                }
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    game.internal_rule_action(
        &json!({"do":"attack","attacker":"source","target":format!("{other}.leader")}),
    )
    .unwrap();
    game.advance_player_input();
    let pending = question(&game, actor).unwrap();
    let Some(DecisionShape::Pending(choices)) = domain(&pending) else {
        panic!("the attack publishes its acquired Drive occurrence");
    };
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: pending.token,
                answer: player::Answer::Pending(choices.first().unwrap().handle)
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    assert!(
        matches!(
            domain(&question(&game, actor).unwrap()),
            Some(DecisionShape::DriveTrigger { .. })
        ),
        "the revealed icon produces a Drive stop"
    );
    game
}

#[test]
fn drive_designation_and_acceptance_reach_the_native_stop_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        for (name, expected, required) in [
            ("critical", wire::TriggerIcon::Critical, true),
            ("stand", wire::TriggerIcon::Stand, true),
            ("draw", wire::TriggerIcon::Draw, false),
            ("heal", wire::TriggerIcon::Heal, false),
        ] {
            for accept in [false, true] {
                let mut game = make(actor, name);
                let before = board_fixtures::exact_authority(&game);
                let (native, usage, shape, binding) = projected(&game, actor);
                assert!(
                    board_fixtures::exact_authority(&game) == before,
                    "Drive projection preserves authority"
                );
                assert_eq!(usage, wire::QuestionUse::DriveTrigger);
                let wire::QuestionShape::DriveTrigger {
                    icon,
                    allowed,
                    targets,
                    target_bounds,
                } = shape
                else {
                    panic!("Drive retains its joint answer");
                };
                assert_eq!(icon, expected);
                assert_eq!(allowed, vec![false, true]);
                assert_eq!(target_bounds.min, WireNat::from(u32::from(required)));
                assert_eq!(target_bounds.max, Some(target_bounds.min));
                assert_eq!(targets.len(), usize::from(required));
                let target = (accept && required).then(|| targets.first().unwrap().id.clone());
                let answer = Answer::DriveTrigger { accept, target };
                submit_exact(&mut game, &native, &binding, &answer);
                let state = game.projection(View::Referee).unwrap();
                match name {
                    "critical" => assert_eq!(
                        state["objects"]["source"]["power"],
                        if accept { 4_u32 } else { 2 }
                    ),
                    "stand" => assert_eq!(state["objects"]["source"]["acted"], !accept),
                    "draw" => assert_eq!(
                        state[seats(actor).0]["hand"].as_array().unwrap().len(),
                        usize::from(accept)
                    ),
                    "heal" => assert_eq!(
                        state[seats(actor).0]["leader"]["life"],
                        if accept { 13_u32 } else { 10 }
                    ),
                    _ => panic!("four native icons"),
                }
            }
        }
    }
}

#[test]
fn drive_refuses_wrong_tags_foreign_or_missing_targets_and_targets_on_decline() {
    let game = make(Viewer::P1, "critical");
    let (native, _, shape, binding) = projected(&game, Viewer::P1);
    let wire::QuestionShape::DriveTrigger { targets, .. } = shape else {
        panic!("Drive");
    };
    let target = targets.first().unwrap();
    assert!(
        matches!(target.subject.as_ref(),Some(sve_battle_contract::Subject::Card {card,..}) if card.face==WireNat::from(1_u32)),
        "the same board supplies the mapped printing face"
    );
    for answer in [
        Answer::Confirm { accept: true },
        Answer::DriveTrigger {
            accept: true,
            target: None,
        },
        Answer::DriveTrigger {
            accept: false,
            target: Some(target.id.clone()),
        },
        Answer::DriveTrigger {
            accept: true,
            target: Some(sve_battle_contract::OptionId::new("foreign".into())),
        },
    ] {
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
    let mut wrong = components(&game, Viewer::P1);
    wrong.viewer = sve_battle_contract::Seat::P2;
    assert!(matches!(
        project_question(&native, &wrong, &QuestionId::new("wrong".into())),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn missing_capabilities_duplicate_coordinates_and_unsupported_icons_refuse_the_whole_drive() {
    let game = make(Viewer::P1, "critical");
    let board = components(&game, Viewer::P1);
    for mode in [0_u32, 1, 2, 3] {
        let mut native = question(&game, Viewer::P1).unwrap();
        let player::QuestionShape::Decision(DecisionShape::DriveTrigger { icon, targets }) =
            &mut native.shape
        else {
            panic!("Drive");
        };
        match mode {
            0 => targets.first_mut().unwrap().object = None,
            1 => targets.push(targets.first().unwrap().clone()),
            2 => *icon = TriggerIcon::Front,
            3 => *icon = TriggerIcon::Draw,
            _ => panic!("four malformed domains"),
        }
        assert!(matches!(
            project_question(&native, &board, &QuestionId::new("missing".into())),
            Err(ProjectionError::Unavailable)
        ));
    }
}

#[test]
fn empty_designations_keep_acceptance_and_zero_target_bounds() {
    let game = make(Viewer::P1, "critical");
    for icon in [TriggerIcon::Critical, TriggerIcon::Stand] {
        let mut native = question(&game, Viewer::P1).unwrap();
        native.shape = player::QuestionShape::Decision(DecisionShape::DriveTrigger {
            icon,
            targets: vec![],
        });
        let (_, shape, binding) = project_question(
            &native,
            &components(&game, Viewer::P1),
            &QuestionId::new("empty".into()),
        )
        .unwrap();
        assert!(matches!(
            shape,
            wire::QuestionShape::DriveTrigger {
                target_bounds: wire::Bounds {
                    min: WireNat::ZERO,
                    max: Some(WireNat::ZERO)
                },
                ..
            }
        ));
        assert_eq!(
            binding.decode(&Answer::DriveTrigger {
                accept: true,
                target: None
            }),
            Ok(player::Answer::DriveTrigger {
                accept: true,
                target: None
            })
        );
    }
}
