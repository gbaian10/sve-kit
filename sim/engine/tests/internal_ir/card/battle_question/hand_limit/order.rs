use super::*;
use sve_battle_contract::CardRef;

fn expected_faces(reverse: bool) -> Vec<CardRef> {
    let mut faces = vec![
        CardRef {
            printing_id: "printing-follower".into(),
            face: Nat::from(1_u32),
        },
        CardRef {
            printing_id: "printing-spell".into(),
            face: Nat::from(0_u32),
        },
    ];
    if reverse {
        faces.reverse();
    }
    faces
}

fn assert_public_placement(game: &Game, actor: Viewer, reverse: bool) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let board = game
            .battle_observation(viewer, &board_fixture::fixture().unwrap().references)
            .unwrap();
        let cemetery = &board
            .players
            .iter()
            .find(|player| player.seat == seat(actor))
            .unwrap()
            .zones
            .cemetery;
        assert_eq!(
            cemetery.count.get(),
            2,
            "both selected cards enter the cemetery"
        );
        let faces = cemetery
            .cards
            .iter()
            .map(|member| board.objects[&member.object].effective_face.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            faces,
            expected_faces(reverse),
            "public placement preserves the owner's chosen order"
        );
        assert_eq!(
            cemetery
                .cards
                .iter()
                .map(|member| member.position)
                .collect::<Vec<_>>(),
            vec![Some(Nat::from(0_u32)), Some(Nat::from(1_u32))],
            "positions describe the reported placement, not a maintained cemetery game order"
        );
        let movements = game
            .events(viewer, None)
            .unwrap()
            .filter(|event| {
                matches!(
                    event.kind,
                    EventKind::Movement {
                        method: MovementMethod::Discard,
                        ..
                    }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(movements.len(), 2, "one rule discard moves both cards");
        let group = movements.first().unwrap().group;
        assert!(
            movements.iter().all(|event| event.group == group),
            "choosing placement order does not split the simultaneous discard"
        );
    }
}

#[test]
fn wire_hand_limit_preserves_the_owners_placement_order_for_distinct_printings() {
    for actor in [Viewer::P1, Viewer::P2] {
        for reverse in [false, true] {
            let mut hand = vec!["unit-amulet"; 7];
            hand.extend(["unit-follower", "unit-spell"]);
            let mut game = make_closing_with_hand(actor, &hand, "unit-amulet");
            let native = player_pipeline::question(&game, actor);
            let QuestionShape::Closing(ClosingQuestion::Discard { cards, count }) = native.shape
            else {
                panic!("native hand-limit question")
            };
            assert_eq!(count, 2);
            let projection = project(&game, actor, "host-discard-placement");
            assert_eq!(
                assert_candidates(&projection, &cards, actor, count),
                Some(())
            );
            let mut choices = candidates(&projection)
                .unwrap()
                .iter()
                .skip(7)
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>();
            let mut expected = cards
                .iter()
                .skip(7)
                .map(|card| card.handle.clone())
                .collect::<Vec<_>>();
            assert_eq!(choices.len(), 2);
            if reverse {
                choices.reverse();
                expected.reverse();
            }
            let command = projection
                .answer(actor, &projection.question().id, &select(choices))
                .unwrap();
            let Command::Closing {
                token,
                answer: ClosingAnswer::Discard(handles),
            } = &command
            else {
                panic!("native closing discard")
            };
            assert_eq!(*token, native.token);
            assert_eq!(
                *handles, expected,
                "the answer specifies the owner's placement order"
            );
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert_discarded_hand(&game, actor, &cards, count);
            assert_public_placement(&game, actor, reverse);
        }
    }
}
