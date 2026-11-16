use super::*;

#[test]
fn transform_templates_keep_the_original_destination_and_assignment_channel() {
    for actor in [Viewer::P1, Viewer::P2] {
        for destination in [
            actor,
            if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            },
        ] {
            let body = json!({"cost":{"pp":1_u32},"targets":[{"bind":"doll","from":{"who":if destination == actor { "self" } else { "opp" },"zone":"ex"},"n":1_u32}],"do":[{"transform":{"ref":"doll","into":{"allocate":{"among":["capacity-a","capacity-b"],"each_exactly_once":true}}}},{"life":{"who":"self","delta":1_u32}}]});
            let mut own = json!({"field":[{"id":"source","card":"unit-follower"}]});
            let mut other = json!({});
            let doll = json!([{"id":"doll","card":"unit-follower"}]);
            if destination == actor {
                own["ex"] = doll;
            } else {
                other["ex"] = doll;
            }
            let fixture = fixture::fixture();
            let mut game = fixture::game_with(actor, &program(body), own, other, fixture);
            let (target, target_shape, target_binding) =
                fixture::project_game(&game, actor, "target");
            let wire::QuestionShape::Select { selection } = target_shape else {
                panic!("target");
            };
            submit_exact(
                &mut game,
                &target,
                &target_binding,
                &Answer::Select {
                    selected: selection
                        .candidates
                        .iter()
                        .map(|item| item.id.clone())
                        .collect(),
                    groups: None,
                    roles: vec![],
                },
            );
            let before = board_fixtures::exact_authority(&game);
            let (native, shape, binding) = fixture::project_game(&game, actor, "templates");
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "template previews allocate nothing"
            );
            assert_eq!(native.actor, actor);
            let wire::QuestionShape::Choose {
                options, capacity, ..
            } = shape
            else {
                panic!("template assignment");
            };
            assert_eq!(
                capacity.unwrap(),
                wire::Capacity {
                    destination: wire::Destination {
                        player: seat(destination),
                        zone: Zone::Ex,
                        position: None
                    },
                    available: WireNat::from(1_u32)
                }
            );
            assert_eq!(options.len(), 2);
            let last = options.last().unwrap();
            let wire::ChoiceValue::Tokens { tokens } = &last.value else {
                panic!("prospective tokens");
            };
            assert_eq!(tokens.len(), 1);
            assert_eq!(
                tokens[0].card.as_ref().unwrap().printing_id,
                "capacity-print-capacity-token-b"
            );
            assert_eq!(tokens[0].card.as_ref().unwrap().face, WireNat::from(1_u32));
            submit_exact(
                &mut game,
                &native,
                &binding,
                &Answer::Choose {
                    chosen: vec![last.id.clone()],
                },
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seats(destination).0]["banish"], json!(["doll"]));
            let ex = state[seats(destination).0]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1);
            assert_eq!(
                state["objects"][ex[0].as_str().unwrap()]["card"],
                "capacity-token-b"
            );
            assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
        }
    }
}

#[test]
fn same_rules_name_does_not_reselect_a_different_printing() {
    let fixture = fixture::shared_fixture();
    assert_eq!(
        fixture.catalog.token_print("capacity-shared").unwrap(),
        "capacity-token-b",
        "a label lookup would select the wrong printing"
    );
    let body = json!({"cost":{"pp":1_u32},"do":[{"create":{"token":["capacity-a","capacity-b","capacity-c"],"n":1_u32,"to":"ex"}}]});
    for actor in [Viewer::P1, Viewer::P2] {
        let own = json!({"field":[{"id":"source","card":"unit-follower"}],"ex":(0_u32..3).map(|index| json!({"id":format!("old-{index}"),"card":"unit-follower"})).collect::<Vec<_>>()});
        let mut game = fixture::game_with(actor, &program(body.clone()), own, json!({}), fixture);
        let (native, shape, binding) = fixture::project_with(&game, actor, "same-label", fixture);
        let wire::QuestionShape::Choose { options, .. } = shape else {
            panic!("native capacity");
        };
        let option = options.iter().find(|option| matches!(&option.value, wire::ChoiceValue::Tokens { tokens } if tokens.iter().any(|token| token.rules_name == "capacity-c"))).unwrap();
        let wire::ChoiceValue::Tokens { tokens } = &option.value else {
            panic!("tokens");
        };
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.card.as_ref().unwrap().printing_id.as_str())
                .collect::<Vec<_>>(),
            [
                "capacity-print-capacity-token-a",
                "capacity-print-capacity-token-c"
            ]
        );
        submit_exact(
            &mut game,
            &native,
            &binding,
            &Answer::Choose {
                chosen: vec![option.id.clone()],
            },
        );
        let state = game.projection(View::Referee).unwrap();
        let created = state[seats(actor).0]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|id| {
                let card = state["objects"][id.as_str().unwrap()]["card"]
                    .as_str()
                    .unwrap();
                card.starts_with("capacity-token-").then_some(card)
            })
            .collect::<Vec<_>>();
        assert_eq!(created, ["capacity-token-a", "capacity-token-c"]);
    }
}
