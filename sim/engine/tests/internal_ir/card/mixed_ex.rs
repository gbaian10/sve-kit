#![expect(
    clippy::panic,
    reason = "Unexpected synthetic trigger domains fail the regression."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn batch() -> Value {
    json!({"simultaneous":{"do":[
        {"move":{"ref":"this","to":"ex","result":"returned"}},
        {"create":{"token":"unit-spell","to":"ex","n":1}}
    ]}})
}

fn tree(stale: bool) -> Value {
    let mut heads = vec![(
        "activate",
        json!({"cost":{"pp":0},"do":[{"destroy":"this"}]}),
    )];
    if stale {
        heads.push((
            "lastword",
            json!({"do":[{"move":{"ref":"this","to":"ex"}}]}),
        ));
    }
    heads.push((
        "lastword",
        json!({"do":[batch(),
            {"counter":{"on":{"follow":"returned"},"name":"test","delta":2}}
        ]}),
    ));
    evolution_linked_movement::tree(&heads)
}

fn board(program: Option<&WellFormedCardProgram>, free: usize) -> Game {
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["card_type"] = json!("スペル・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..5_usize.saturating_sub(free))
            .map(|index| json!({"id":format!("old-{index}"),"card":"unit-amulet"}))
            .collect::<Vec<_>>()
    );
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "mixed-capacity",
    )
    .unwrap();
    board
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    board
        .load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    if let Some(program) = program {
        board.load_internal_card("unit-follower", program).unwrap();
    }
    board
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn fire(board: &mut Game, line: u64) -> CardPlay {
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("Lastword");
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

#[test]
fn mixed_ex_receipts_publish_one_atomic_batch_without_a_fake_token_movement_fact() {
    let program = lowerer()
        .lower(&tree(false))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut board = board(Some(&program), 2);
    resolved(
        board
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
    );
    checks(&mut board);
    let result = resolved(fire(&mut board, 2));
    let batch = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Batch(batch) = outcome {
                Some(batch)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        batch.receipts.len(),
        2,
        "the parent names both completed children"
    );
    assert_eq!(
        batch.successful.len(),
        2,
        "the physical move and creation both succeed"
    );
    assert_eq!(
        batch
            .successful
            .iter()
            .filter(|member| member.before.is_none())
            .count(),
        1,
        "a newly allocated token has no prior zone or physical generation"
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Movement(_)))
            .count(),
        1,
        "only the original physical base produces a movement fact"
    );
    let token = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        token.created.len(),
        1,
        "the creation child exports its live after generation"
    );
    assert_eq!(
        token.created[0].generation,
        Generation(1),
        "void allocation is committed once"
    );
}

#[test]
fn mixed_ex_stale_departure_does_not_compete_with_or_follow_the_new_token() {
    let program = lowerer()
        .lower(&tree(true))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut board = board(Some(&program), 2);
    resolved(
        board
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
    );
    checks(&mut board);
    resolved(fire(&mut board, 2));
    checks(&mut board);
    let result = resolved(fire(&mut board, 3));
    checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["s"]["generation"], 2,
        "a stale Lastword cannot move the later EX generation"
    );
    assert_eq!(
        after["P1"]["ex"].as_array().unwrap().len(),
        5,
        "only the token is eligible for the remaining space"
    );
    assert_eq!(
        after["objects"]["s"]["counters"],
        json!({}),
        "the failed move grants no follow authority"
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Movement(_))),
        "creation is not a cemetery movement"
    );
    let batch = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Batch(batch) = outcome {
                Some(batch)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        batch.successful.len(),
        1,
        "the batch records only the successful creation"
    );
}

#[test]
fn mixed_ex_lowering_keeps_other_batches_and_sibling_dependencies_closed() {
    lowerer().lower(&tree(false)).unwrap();
    for alternative in [
        json!({"simultaneous":{"do":[]}}),
        json!({"simultaneous":{"do":[{"move":{"ref":"this","to":"hand"}}, {"create":{"token":"unit-spell","to":"ex","n":1}}]}}),
        json!({"simultaneous":{"do":[{"move":{"ref":"this","to":"ex"}}, {"create":{"token":"unit-spell","to":"ex","n":2}}]}}),
        json!({"simultaneous":{"do":[{"move":{"ref":"this","to":"ex","result":"returned"}}, {"counter":{"on":{"follow":"returned"},"name":"test","delta":1}}]}}),
        json!({"simultaneous":{"do":[{"move":{"ref":"this","to":"ex"}}, {"create":{"token":"unit-spell","to":"field","n":1}}]}}),
    ] {
        let mut input = tree(false);
        input["cards"]["unit-follower"]["abilities"][1]["lastword"]["do"] = json!([alternative]);
        assert!(
            lowerer().lower(&input).is_err(),
            "only the required closed pair is lowered"
        );
    }
}

#[test]
fn mixed_ex_raw_ir_cannot_bypass_destination_count_or_batch_membership_guards() {
    let program = lowerer()
        .lower(&tree(false))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0..4 {
        let mut raw = program.card().clone();
        let Node::Control {
            control: Control::Simultaneous { batch, .. },
        } = &mut raw.abilities[1].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("batch");
        };
        match case {
            0 => {
                if let Action::Move { movement, .. } = &mut batch.members[0].node.action {
                    movement.to = MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    };
                }
            }
            1 => {
                if let Action::Token { tokens, .. } = &mut batch.members[1].node.action {
                    tokens[0].count = Expr::Literal(Nat(2));
                }
            }
            2 => {
                if let Action::Token {
                    destination:
                        TokenDestination::Single {
                            zone: Expr::Literal(zone),
                        },
                    ..
                } = &mut batch.members[1].node.action
                {
                    zone.player = PlayerRef::Owner {};
                }
            }
            _ => batch.members.swap(0, 1),
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut board = board(None, 2);
        let before = exact_authority(&board);
        board
            .load_internal_card("unit-follower", &checked)
            .unwrap_err();
        assert_eq!(
            exact_authority(&board),
            before,
            "invalid typed batches fail atomically at load"
        );
    }
}
