use super::super::select::submit_projected;
use super::*;
use sve_battle_contract::{Nat as WireNat, ObjectHandle};

pub(super) fn game(actor: Viewer, zone: &str) -> Game {
    let body = json!({"cost":{"pp":1_u32},"do":[{"create":{"token":["capacity-a","capacity-b"],"n":2_u32,"to":zone}},{"life":{"who":"self","delta":1_u32}}]});
    let mut zones = json!({"field":[{"id":"source","card":"unit-follower"}]});
    let mut occupied = (0_u32..3)
        .map(|index| json!({"id":format!("old-{index}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    if zone == "field" {
        occupied.remove(0);
        occupied.insert(0, json!({"id":"source","card":"unit-follower"}));
    }
    zones[zone] = json!(occupied);
    fixture::game(actor, &body, zones)
}

#[test]
fn prospective_token_combinations_keep_exact_printings_and_create_nothing_until_answered() {
    for actor in [Viewer::P1, Viewer::P2] {
        for zone in ["field", "ex"] {
            let mut game = game(actor, zone);
            let captured = question(&game, actor).unwrap();
            assert!(
                matches!(domain(&captured), Some(DecisionShape::TokenCapacity(_))),
                "actual native token capacity stop"
            );
            let before = board_fixtures::exact_authority(&game);
            let old_ids = game.projection(View::Referee).unwrap()[seats(actor).0][zone]
                .as_array()
                .unwrap()
                .clone();
            let before_objects = fixture::board(&game, actor)
                .objects
                .keys()
                .cloned()
                .collect::<Vec<ObjectHandle>>();
            let native = captured;
            let id = QuestionId::new("tokens".into());
            let binding = game
                .battle_question(actor, &id, &fixture::fixture().references)
                .unwrap()
                .unwrap();
            let shape = binding.question().shape.clone();
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "a prospective printing creates no object and consumes no authority"
            );
            assert_eq!(
                fixture::board(&game, actor)
                    .objects
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                before_objects
            );
            let wire::QuestionShape::Choose {
                options,
                bounds,
                count,
                earlier,
                capacity,
            } = shape
            else {
                panic!("complete token combinations");
            };
            assert_eq!(options.len(), 3);
            assert_eq!(bounds.min, WireNat::from(1_u32));
            assert_eq!(bounds.max, Some(bounds.min));
            assert_eq!(
                capacity,
                Some(wire::Capacity {
                    destination: wire::Destination {
                        player: seat(actor),
                        zone: if zone == "field" {
                            Zone::Field
                        } else {
                            Zone::Ex
                        },
                        position: None
                    },
                    available: WireNat::from(2_u32)
                })
            );
            assert_eq!(count, None);
            assert_eq!(earlier, []);
            let last = options.last().unwrap();
            let wire::ChoiceValue::Tokens { tokens } = &last.value else {
                panic!("typed token set");
            };
            assert_eq!(tokens.len(), 2);
            assert!(tokens.iter().all(|token| {
                token.rules_name == "capacity-b"
                    && token.card.as_ref().is_some_and(|card| {
                        card.printing_id == "capacity-print-capacity-token-b"
                            && card.face == WireNat::from(1_u32)
                    })
            }));
            let answer = Answer::Choose {
                chosen: vec![last.id.clone()],
            };
            assert!(matches!(
                binding
                    .answer(native.actor, &binding.question().id, &answer)
                    .unwrap(),
                player::Command::Answer {
                    answer: player::Answer::TokenCapacity(2),
                    ..
                }
            ));
            submit_projected(&mut game, &native, &binding, &answer);
            assert_committed(&game, actor, zone, &old_ids);
        }
    }
}

fn assert_committed(game: &Game, actor: Viewer, zone: &str, old_ids: &[Value]) {
    let after_board = fixture::board(game, actor);
    let created_faces = after_board
        .objects
        .values()
        .filter(|object| object.card.printing_id == "capacity-print-capacity-token-b")
        .map(|object| &object.card)
        .collect::<Vec<_>>();
    assert_eq!(created_faces.len(), 2);
    assert!(
        created_faces
            .iter()
            .all(|card| card.face == WireNat::from(1_u32)),
        "real creation uses the previewed native face, mapped through permanent references"
    );
    let state = game.projection(View::Referee).unwrap();
    let created = state[seats(actor).0][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| !old_ids.contains(id))
        .collect::<Vec<_>>();
    assert_eq!(created.len(), 2);
    assert!(
        created
            .iter()
            .all(|id| state["objects"][id.as_str().unwrap()]["card"] == "capacity-token-b")
    );
    assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
}
