use super::*;

pub(super) fn pilot(id: u64) -> Located<Node> {
    instruction(
        id,
        Action::BoxPilot {
            subject: ObjectOperand::SourceHolder {},
            mode: BoxPilotMode::Pilot {
                duration: Duration {
                    end: DurationEnd::CurrentTurnEnd {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
            },
        },
    )
}

pub(super) fn board(kind: &str, attack: &str, life: &str) -> Game {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut registry: Value = serde_json::from_str(&registry).unwrap();
    for (name, japanese) in [("aura", "オーラ"), ("storm", "疾走")] {
        registry["keywords"][name] =
            json!({"ja":japanese,"rule":"synthetic","expansion":{"op":"keyword","name":name}});
    }
    let registry = registry.to_string();
    let mut cards = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    cards[0]["faces"][0]["card_type"] = json!(kind);
    cards[0]["faces"][0]["power"] = json!(attack);
    cards[0]["faces"][0]["hp"] = json!(life);
    let snapshot = cards
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("pilot.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-spell","state":{"entered_this_turn":true}}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..12)
                .map(|index| json!({"id":format!("{seat}-{index}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    Game::new(catalog, &initial, &Value::Null, &Value::Null, "pilot").unwrap()
}

fn run(board: &mut Game, steps: Vec<Located<Node>>) -> Progress {
    board
        .execute_internal(&checked(steps), &source("synthetic"), context(1))
        .unwrap()
}

fn end(board: &mut Game) {
    board.decide(&json!({"do":"end-phase"}), "end").unwrap();
    board.decide(&json!({"do":"pass"}), "pass").unwrap();
}

#[test]
fn pilot_requires_current_amulet_with_both_printed_stats() {
    for (kind, attack, life) in [
        ("アミュレット", "-", "3"),
        ("アミュレット", "3", "-"),
        ("フォロワー", "3", "3"),
    ] {
        let mut board = board(kind, attack, life);
        let before = board.projection(View::Referee).unwrap()["objects"]["s"].clone();
        let result = run(&mut board, vec![pilot(1)]);
        assert_eq!(
            result.receipts[&ReceiptId(1)],
            InstructionOutcome::Piloted(vec![])
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["s"],
            before
        );
    }
}

#[test]
fn pilot_keeps_token_kind_and_repeated_instruction_preserves_modifications() {
    let mut board = board("アミュレット・トークン", "3", "3");
    let result = run(
        &mut board,
        vec![
            pilot(1),
            numeric::modify(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                numeric::pair(1, 1),
                DurationEnd::NoTemporalExpiry {},
            ),
            pilot(3),
        ],
    );
    assert!(
        matches!(&result.receipts[&ReceiptId(1)], InstructionOutcome::Piloted(subjects) if subjects.len()==1)
    );
    assert_eq!(
        result.receipts[&ReceiptId(3)],
        InstructionOutcome::Piloted(vec![])
    );
    let projection = board.projection(View::Referee).unwrap();
    assert_eq!(
        projection["objects"]["s"]["card_type"],
        "フォロワー・トークン"
    );
    assert_eq!(projection["objects"]["s"]["power"], 4);
    assert_eq!(projection["objects"]["s"]["hp"], 4);
}

#[test]
fn pilot_expiry_discards_damage_and_longer_numeric_modifiers() {
    let mut board = board("アミュレット・トークン", "3", "3");
    run(
        &mut board,
        vec![
            pilot(1),
            numeric::modify(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                numeric::pair(1, 1),
                DurationEnd::NoTemporalExpiry {},
            ),
            damage(
                3,
                RecipientSet::Objects {
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                },
                2,
            ),
        ],
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["s"]["hp"],
        2
    );
    end(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["s"]["card_type"],
        Value::Null
    );
    assert!(
        board.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("s")),
        "expiry does not destroy the former follower"
    );
    end(&mut board);
    run(&mut board, vec![pilot(1)]);
    let projection = board.projection(View::Referee).unwrap();
    assert_eq!(projection["objects"]["s"]["power"], 3);
    assert_eq!(projection["objects"]["s"]["hp"], 3);
    assert_eq!(projection["objects"]["s"]["max_hp"], 3);
    assert_eq!(projection["objects"]["s"]["damage"], 0);
}

#[test]
fn pilot_does_not_apply_old_expiry_after_the_source_changes_generation() {
    let mut board = board("アミュレット", "3", "3");
    run(
        &mut board,
        vec![
            pilot(1),
            movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            ),
        ],
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
    end(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["objects"]["s"]["hp"], 3);
    assert_eq!(state["objects"]["s"]["card_type"], Value::Null);
}

#[test]
fn pilot_keeps_boxed_and_unrequested_duration_shapes_closed() {
    let mut step = pilot(1);
    if let Node::Instruction { instruction } = &mut step.node {
        instruction.action = Action::BoxPilot {
            subject: ObjectOperand::SourceHolder {},
            mode: BoxPilotMode::Boxed {
                duration: Duration {
                    end: DurationEnd::CurrentTurnEnd {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                cutoff: TimestampBasis::ZoneEntry {
                    object: ObjectOperand::SourceHolder {},
                },
            },
        };
    }
    WellFormedProgram::check(body(1, vec![step])).unwrap_err();
    for end in [
        DurationEnd::NoTemporalExpiry {},
        DurationEnd::EndResolution {},
    ] {
        let mut step = pilot(1);
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::BoxPilot {
                mode: BoxPilotMode::Pilot { duration },
                ..
            } = &mut instruction.action
        {
            duration.end = end;
        }
        WellFormedProgram::check(body(1, vec![step])).unwrap_err();
    }
}
