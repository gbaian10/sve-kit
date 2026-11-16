use super::*;
use crate::battle::tests::payment_fixtures;
use crate::ir::{lowering::ExpandedLowerer, payment_expanded_tree};
use sve_battle_contract::Subject;

fn mixed_program() -> WellFormedCardProgram {
    let mut document = payment_fixtures::x_document(
        payment_expanded_tree(),
        "activate",
        json!({"cost":{"pp":0_u32},"do":[{"destroy":"this"}]}),
    )
    .unwrap();
    let mut lastword = document["cards"]["unit-follower"]["abilities"][0].clone();
    lastword.as_object_mut().unwrap().remove("activate");
    lastword["line"] = json!(2_u32);
    lastword["lastword"] = json!({"do":[{"simultaneous":{"do":[{"move":{"ref":"this","to":"ex","result":"returned"}},{"create":{"token":"capacity-a","to":"ex","n":1_u32}}]}}]});
    document["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(lastword);
    ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn game(actor: Viewer, free: usize) -> Game {
    let zones = json!({"field":[{"id":"source","card":"unit-follower"}],"ex":(0..5_usize.checked_sub(free).unwrap()).map(|index| json!({"id":format!("old-{index}"),"card":"unit-amulet"})).collect::<Vec<_>>()});
    let mut game = fixture::game_with(
        actor,
        &mixed_program(),
        zones,
        json!({}),
        fixture::fixture(),
    );
    let pending = question(&game, actor).unwrap();
    let Some(DecisionShape::Pending(choices)) = domain(&pending) else {
        panic!("real Lastword");
    };
    assert_eq!(choices.len(), 1);
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: pending.token,
                answer: player::Answer::Pending(choices[0].handle)
            }
        )
        .kind,
        player::SubmissionKind::Applied
    );
    game
}

#[test]
fn mixed_arrivals_keep_existing_objects_separate_from_prospective_tokens() {
    for actor in [Viewer::P1, Viewer::P2] {
        for keep_token in [false, true] {
            let mut game = game(actor, 1);
            let (native, shape, binding) = fixture::project_game(&game, actor, "mixed");
            let wire::QuestionShape::Select { selection } = shape else {
                panic!("mixed capacity");
            };
            assert_eq!(selection.candidates.len(), 2);
            assert_eq!(selection.capacity.unwrap().available, WireNat::from(1_u32));
            let existing = selection
                .candidates
                .iter()
                .find(|candidate| {
                    matches!(
                        candidate.subject,
                        Some(Subject::Card {
                            object: Some(_),
                            ..
                        })
                    )
                })
                .unwrap();
            let token = selection
                .candidates
                .iter()
                .find(|candidate| {
                    matches!(candidate.subject, Some(Subject::Card { object: None, .. }))
                })
                .unwrap();
            assert_eq!(existing.controller, Some(seat(actor)));
            assert_eq!(existing.origin.unwrap().zone, Zone::Cemetery);
            assert_eq!(token.controller, None);
            assert_eq!(token.origin, None);
            let Some(Subject::Card { card, .. }) = &token.subject else {
                panic!("token face");
            };
            assert_eq!(card.printing_id, "capacity-print-capacity-token-a");
            assert_eq!(card.face, WireNat::from(1_u32));
            let selected = if keep_token { &token.id } else { &existing.id };
            submit_exact(
                &mut game,
                &native,
                &binding,
                &Answer::Select {
                    selected: vec![selected.clone()],
                    groups: None,
                    roles: vec![],
                },
            );
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state[seats(actor).0][if keep_token { "cemetery" } else { "ex" }]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
            assert_eq!(state[seats(actor).0]["ex"].as_array().unwrap().len(), 5);
            assert_eq!(
                state["objects"]
                    .as_object()
                    .unwrap()
                    .values()
                    .filter(|object| object["card"] == "capacity-token-a")
                    .count(),
                usize::from(keep_token)
            );
        }
    }
}

#[test]
fn no_capacity_question_is_invented_for_zero_space_or_a_single_native_outcome() {
    for actor in [Viewer::P1, Viewer::P2] {
        for free in [0, 2] {
            let game = game(actor, free);
            assert!(
                !question(&game, actor)
                    .as_ref()
                    .is_some_and(|native| matches!(
                        domain(native),
                        Some(DecisionShape::MovementCapacity(_) | DecisionShape::TokenCapacity(_))
                    )),
                "native automatic arrival outcome needs no answer"
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seats(actor).0]["ex"].as_array().unwrap().len(), 5);
            assert!(
                state[seats(actor).0][if free == 0 { "cemetery" } else { "ex" }]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
        }
        let body = json!({"cost":{"pp":0_u32},"do":[{"create":{"token":"capacity-a","n":2_u32,"to":"ex"}}]});
        for occupied in [3_u32, 4, 5] {
            let zones = json!({"field":[{"id":"source","card":"unit-follower"}],"ex":(0_u32..occupied).map(|index| json!({"id":format!("old-{index}"),"card":"unit-amulet"})).collect::<Vec<_>>()});
            let game = fixture::game(actor, &body, zones);
            assert!(
                !question(&game, actor)
                    .as_ref()
                    .is_some_and(|native| matches!(
                        domain(native),
                        Some(DecisionShape::MovementCapacity(_) | DecisionShape::TokenCapacity(_))
                    )),
                "one native token combination is automatic, including zero capacity"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["ex"]
                    .as_array()
                    .unwrap()
                    .len(),
                5
            );
        }
    }
}
