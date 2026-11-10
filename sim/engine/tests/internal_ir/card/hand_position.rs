#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the combined private choice."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, TargetRoleAssignment};

fn steps() -> Value {
    json!([
        {"choose":{"bind":"picked","from":{"who":"self","zone":"hand"},"n":1,"position":"top_or_bottom"}},
        {"move":{"ref":"picked","to":"deck","position":{"chosen":"picked"}}}
    ])
}

fn tree(body: &Value) -> Value {
    evolution_linked_movement::tree(&[("activate", json!({"cost":{"pp":0},"do":body}))])
}

fn program() -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(&steps()))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(seat: u32, empty: bool) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["hand"] = if empty {
        json!([])
    } else {
        json!([{"id":"h","card":"unit-spell"},{"id":"other","card":"unit-amulet"}])
    };
    initial["players"][actor]["zones"]["deck"] = json!([
        {"id":"first","card":"unit-follower"},{"id":"last","card":"unit-follower"}
    ]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "private-position",
    )
    .unwrap();
    board
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    board
        .load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    board
        .load_internal_card("unit-follower", &program())
        .unwrap();
    board
}

fn question(board: &mut Game, seat: u32) -> DecisionView {
    let program = program();
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("s", program.card().abilities[0].node.id, PlayerId { seat })
        .unwrap()
    else {
        panic!("combined card and edge");
    };
    question
}

fn answer(question: &DecisionView, selected: &[u64], top: bool) -> Answer {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private targets");
    };
    Answer::TargetsWithRoles {
        targets: selected.to_vec(),
        roles: domain
            .roles
            .iter()
            .map(|role| TargetRoleAssignment {
                role: role.role,
                targets: if role.position
                    == Some(if top {
                        Position::Top {}
                    } else {
                        Position::Bottom {}
                    }) {
                    selected.to_vec()
                } else {
                    vec![]
                },
            })
            .collect(),
    }
}

#[test]
fn hand_position_chooses_one_private_card_and_edge_in_the_same_answer_both_seats() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let other = if seat == 1 { View::P2 } else { View::P1 };
        for top in [true, false] {
            let mut board = board(seat, false);
            let question = question(&mut board, seat);
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("private targets");
            };
            assert_eq!(domain.members.len(), 2, "both own-hand cards are available");
            assert_eq!(
                domain
                    .roles
                    .iter()
                    .map(|role| role.position)
                    .collect::<Vec<_>>(),
                vec![Some(Position::Top {}), Some(Position::Bottom {})],
                "positions label the same card question"
            );
            assert!(
                board.internal_decision(other).is_none(),
                "the opponent receives no candidate or edge question"
            );
            let owner = if seat == 1 { View::P1 } else { View::P2 };
            let wire = board.projection(owner).unwrap();
            assert_eq!(
                wire["awaiting"]["domain"]["roles"][0]["position"], "top",
                "the private wire question labels the deck top"
            );
            assert_eq!(
                wire["awaiting"]["domain"]["roles"][1]["position"], "bottom",
                "the private wire question labels the deck bottom"
            );
            let chosen = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("unit-spell"))
                .unwrap()
                .handle;
            let result = resolved(
                board
                    .answer_internal_card(
                        question.actor,
                        question.token,
                        answer(&question, &[chosen], top),
                    )
                    .unwrap(),
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Movement(_)))
                    .count(),
                1,
                "the selected physical card moves once and there is no second question"
            );
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view[actor]["hand"],
                json!(["other"]),
                "the unselected hand card remains"
            );
            assert_eq!(
                view[actor]["deck"],
                if top {
                    json!(["h", "first", "last"])
                } else {
                    json!(["first", "last", "h"])
                },
                "the selected edge controls the actual physical deck order"
            );
            assert_eq!(
                view["objects"]["h"]["generation"], 1,
                "one actual zone change preserves the original object"
            );
            assert!(
                board.projection(other).unwrap()["objects"]["h"].is_null(),
                "the deck coordinate remains private"
            );
        }
    }
}

#[test]
fn hand_position_rejects_missing_edges_foreign_or_duplicate_assignments_atomically() {
    let mut board = board(1, false);
    let question = question(&mut board, 1);
    let before = exact_authority(&board);
    let invalid_roles = [
        vec![],
        vec![TargetRoleAssignment {
            role: Nat(1),
            targets: vec![0],
        }],
        vec![
            TargetRoleAssignment {
                role: Nat(1),
                targets: vec![0],
            },
            TargetRoleAssignment {
                role: Nat(2),
                targets: vec![0],
            },
        ],
        vec![
            TargetRoleAssignment {
                role: Nat(1),
                targets: vec![1],
            },
            TargetRoleAssignment {
                role: Nat(2),
                targets: vec![],
            },
        ],
        vec![
            TargetRoleAssignment {
                role: Nat(99),
                targets: vec![0],
            },
            TargetRoleAssignment {
                role: Nat(2),
                targets: vec![],
            },
        ],
    ];
    for roles in invalid_roles {
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::TargetsWithRoles {
                    targets: vec![0],
                    roles,
                },
            )
            .unwrap_err();
        assert_eq!(
            exact_authority(&board),
            before,
            "invalid joint answers preserve all authority"
        );
    }
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap_err();
    board
        .answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            answer(&question, &[0], true),
        )
        .unwrap_err();
    assert_eq!(
        exact_authority(&board),
        before,
        "neither an omitted position nor the other player can answer"
    );
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                answer(&question, &[0], true),
            )
            .unwrap(),
    );
}

#[test]
fn hand_position_empty_resolution_keeps_the_deck_and_produces_no_movement() {
    let mut board = board(1, true);
    let question = question(&mut board, 1);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("empty hand");
    };
    assert!(
        domain.members.is_empty(),
        "resolution can do as much as possible"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the original exact-one instruction does not invent a card"
    );
    let result = resolved(
        board
            .answer_internal_card(question.actor, question.token, answer(&question, &[], true))
            .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Movement(_))),
        "empty role moves emit no physical movement"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["first", "last"]),
        "the existing deck order is untouched"
    );
}

#[test]
fn hand_position_lowering_requires_the_adjacent_same_binding_and_closed_hand_shape() {
    lowerer().lower(&tree(&steps())).unwrap();
    for case in 0..10 {
        let mut body = steps();
        match case {
            0 => body[1]["move"]["ref"] = json!("another"),
            1 => body[1]["move"]["position"]["chosen"] = json!("another"),
            2 => body[1]["move"]["to"] = json!("hand"),
            3 => body[0]["choose"]["from"]["who"] = json!("opp"),
            4 => body[0]["choose"]["from"]["zone"] = json!("ex"),
            5 => body[0]["choose"]["n"] = json!(2),
            6 => body[0]["choose"]["from"]["kind"] = json!("spell"),
            7 => {
                body.as_array_mut().unwrap().pop();
            }
            8 => body[1]["move"]["position"] = json!("top"),
            _ => {
                body[0]["choose"]
                    .as_object_mut()
                    .unwrap()
                    .remove("position");
            }
        }
        assert!(
            lowerer().lower(&tree(&body)).is_err(),
            "unsupported or unpaired position declarations stay closed"
        );
    }
}

#[test]
fn hand_position_direct_ir_cannot_change_the_role_sources_or_movement_destinations() {
    let original = program();
    for case in 0..5 {
        let mut raw = original.card().clone();
        let body = raw.abilities[0].node.body.as_mut().unwrap();
        match case {
            0 => {
                if let Node::Instruction { instruction } = &mut body.steps[1].node
                    && let Action::Move { movement, .. } = &mut instruction.action
                {
                    movement.to = MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    };
                }
            }
            1 => {
                if let Node::Instruction { instruction } = &mut body.steps[2].node
                    && let Action::Move { movement, .. } = &mut instruction.action
                {
                    movement.position = Some(Position::Top {});
                }
            }
            2 => {
                if let Node::Decision { decision } = &mut body.steps[0].node {
                    decision.visibility = Visibility::Public {};
                }
            }
            3 => {
                if let Node::Decision { decision } = &mut body.steps[0].node
                    && let ChoiceDomain::Objects {
                        target_roles: Some(spec),
                        ..
                    } = &mut decision.domain
                {
                    spec.roles[1].role = Nat(99);
                }
            }
            _ => {
                body.steps.swap(1, 2);
            }
        }
        assert!(
            WellFormedCardProgram::check(raw).is_err(),
            "C1 preserves the checked same-binding position transaction"
        );
    }
}
