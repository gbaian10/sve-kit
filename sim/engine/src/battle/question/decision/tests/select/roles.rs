use super::*;
use crate::battle::question::common::seat;
use sve_battle_contract::RoleChoice;

#[test]
fn private_position_roles_need_the_native_owner_and_deck_destination() {
    let program = program(json!({"cost":{"pp":0_i32},"do":[
        {"choose":{"bind":"picked","from":{"who":"self","zone":"hand"},"n":1_i32,"position":"top_or_bottom"}},
        {"move":{"ref":"picked","to":"deck","position":{"chosen":"picked"}}}
    ]}));
    for actor in [Viewer::P1, Viewer::P2] {
        for top in [true, false] {
            let mut game = board(actor, &program);
            let native = question(&game, actor).unwrap();
            let mut missing_destination = native.clone();
            if let QuestionShape::Decision(DecisionShape::Targets(domain)) =
                &mut missing_destination.shape
            {
                domain.roles.first_mut().unwrap().destination = None;
            }
            assert!(matches!(
                project_question(
                    &missing_destination,
                    &components(&game, actor),
                    &QuestionId::new("missing-edge".into())
                ),
                Err(ProjectionError::Unavailable)
            ));
            let (usage, shape, binding) = project_question(
                &native,
                &components(&game, actor),
                &QuestionId::new("edge".into()),
            )
            .unwrap();
            assert_eq!(usage, wire::QuestionUse::ResolutionSelection);
            let selection = selection(&shape).unwrap();
            let selected = vec![selection.candidates.first().unwrap().id.clone()];
            let edge = if top {
                wire::Edge::Top
            } else {
                wire::Edge::Bottom
            };
            for role in &selection.roles {
                let destination = role.destination.unwrap();
                assert_eq!(destination.player, seat(actor));
                assert_eq!(destination.zone, Zone::Deck);
            }
            let answer = Answer::Select {
                selected: selected.clone(),
                groups: None,
                roles: selection
                    .roles
                    .iter()
                    .map(|role| RoleChoice {
                        role: role.role,
                        selected: if role.destination.unwrap().position == Some(edge) {
                            selected.clone()
                        } else {
                            vec![]
                        },
                    })
                    .collect(),
            };
            let command = binding.answer(native.token, &answer).unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            let state = game.projection(View::Referee).unwrap();
            let deck = state[seats(actor).0]["deck"].as_array().unwrap();
            assert_eq!(deck.len(), 3);
            assert_eq!(
                if top { deck.first() } else { deck.last() },
                Some(&json!("own-0"))
            );
        }
    }
}

#[test]
fn public_edge_roles_use_the_native_owner_when_the_chooser_places_an_opposing_card() {
    let program = program(json!({"cost":{"pp":0_i32},"targets":[
        {"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_i32}
    ],"do":[{"move":{"ref":"victim","to":"deck","position":{"one_of":["top","bottom"],"by":"self"}}}]}));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let formal = question(&game, actor).unwrap();
        let handle = native_selection(&formal)
            .unwrap()
            .members
            .first()
            .unwrap()
            .handle;
        assert_eq!(
            game.submit_player(
                actor,
                player::Command::Answer {
                    token: formal.token,
                    answer: player::Answer::Targets(vec![handle]),
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let native = question(&game, actor).unwrap();
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("other-owner".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::ResolutionSelection);
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let selection = selection(&shape).unwrap();
        assert!(selection.roles.iter().all(|role| {
            role.destination
                .is_some_and(|destination| destination.player == seat(other))
        }));
        let selected = vec![selection.candidates[0].id.clone()];
        let answer = Answer::Select {
            selected: selected.clone(),
            groups: None,
            roles: selection
                .roles
                .iter()
                .map(|role| RoleChoice {
                    role: role.role,
                    selected: if role.destination.unwrap().position == Some(wire::Edge::Top) {
                        selected.clone()
                    } else {
                        vec![]
                    },
                })
                .collect(),
        };
        assert_eq!(
            game.submit_player(actor, binding.answer(native.token, &answer).unwrap())
                .kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).1]["deck"],
            json!(["enemy"])
        );
    }
}
