//! B10 quantity fragments do not admit the corresponding complete production cards.
use super::*;
use sve_engine::game::internal::ResourceOutcome;

fn program(steps: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":steps}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(program: &WellFormedCardProgram, initial: &Value, catalogue: Arc<Catalog>) -> Game {
    let mut board = Game::new(
        catalogue,
        initial,
        &Value::Null,
        &Value::Null,
        "resource-formula",
    )
    .unwrap();
    board.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        board.load_internal_card(number, &card(vec![])).unwrap();
    }
    board
}

fn position(seat: u32) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":1,"max":5});
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial
}

fn activate(board: &mut Game, program: &WellFormedCardProgram, seat: u32) -> Vec<ResourceOutcome> {
    let result = resolved(
        board
            .activate_internal_card("s", program.card().abilities[0].node.id, PlayerId { seat })
            .unwrap(),
    );
    result
        .progress
        .receipts
        .values()
        .filter_map(|receipt| {
            if let InstructionOutcome::Resource(value) = receipt {
                Some(*value)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn ex_named_count_reads_at_the_resource_instruction_and_allows_zero() {
    let program = program(
        &json!([{"pp":{"who":"self","delta":{"count":{"who":"self","zone":"ex","name":"unit-spell"}}}}]),
    );
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for (matching, current, expected) in [(0_u8, 1, 0), (2, 1, 2), (2, 5, 0)] {
            let mut initial = position(seat);
            initial["players"][actor]["pp"]["current"] = json!(current);
            let mut ex = vec![json!({"id":"other","card":"unit-follower"})];
            ex.extend((0..matching).map(|n| json!({"id":format!("x{n}"),"card":"unit-spell"})));
            initial["players"][actor]["zones"]["ex"] = json!(ex);
            let mut board = board(&program, &initial, catalog());
            let receipts = activate(&mut board, &program, seat);
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].requested, Int(i64::from(matching)));
            assert_eq!(receipts[0].actual, Int(expected));
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state[actor]["pp"],
                json!({"current":current+expected,"max":5})
            );
        }
    }
}

#[test]
fn resource_count_formula_observes_a_draw_completed_earlier_in_the_body() {
    let program = program(&json!([
        {"draw":{"n":1}},
        {"pp":{"who":"self","delta":{"plus":[{"count":{"who":"self","zone":"hand"}},1]}}}
    ]));
    let mut initial = position(1);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"unit-spell"}]);
    let mut board = board(&program, &initial, catalog());
    let receipts = activate(&mut board, &program, 1);
    assert_eq!(receipts[0].requested, Int(2));
    assert_eq!(receipts[0].actual, Int(2));
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d"])
    );
}

#[test]
fn resources_read_only_filtered_successful_deck_to_cemetery_members() {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["traits"] = json!(["trait-a"]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalogue = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    for (kind, resource) in [
        ("pp", Resource::PpCurrent),
        ("pp_max", Resource::PpMaximum),
        ("ep", Resource::Ep),
    ] {
        let program = program(&json!([
            {"move":{"top":{"who":"self","n":3},"to":"cemetery","result":"milled"}},
            {kind:{"who":"self","delta":{"count_of":{"receipt":"milled","where":{"trait":"trait-a"},"view":"before"}}}}
        ]));
        for (deck, requested) in [
            (json!([]), 0),
            (json!([{"id":"d0","card":"unit-spell"}]), 1),
            (
                json!([{"id":"d0","card":"unit-spell"},{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-spell"},{"id":"tail","card":"unit-spell"}]),
                2,
            ),
        ] {
            let mut initial = position(1);
            initial["players"]["P1"]["zones"]["deck"] = deck.clone();
            let mut board = board(&program, &initial, Arc::clone(&catalogue));
            let receipts = activate(&mut board, &program, 1);
            assert_eq!(receipts[0].resource, resource);
            assert_eq!(receipts[0].requested, Int(requested));
            assert_eq!(receipts[0].actual, Int(requested));
            if deck.as_array().unwrap().len() > 3 {
                let state = board.projection(View::P2).unwrap();
                assert!(
                    !state["known_cards"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|card| card["id"] == "tail")
                );
            }
        }
    }
}

#[test]
fn resource_author_negative_and_unknown_signed_amounts_are_rejected_at_load() {
    for delta in [
        json!(-1),
        json!({"minus":[0,1]}),
        json!({"minus":[{"count":{"who":"self","zone":"hand"}},1]}),
        json!({"resource":{"who":"self","kind":"pp"}}),
    ] {
        for kind in ["pp", "pp_max", "ep"] {
            let program = program(&json!([{kind:{"who":"self","delta":delta}}]));
            let mut board = Game::new(
                catalog(),
                &position(1),
                &Value::Null,
                &Value::Null,
                "negative-resource",
            )
            .unwrap();
            let before = exact_authority(&board);
            assert!(matches!(
                board.load_internal_card("unit-follower", &program),
                Err(sve_engine::EngineFailure::Unsupported(_))
            ));
            assert_eq!(exact_authority(&board), before);
        }
    }
}

#[test]
fn resource_author_entry_does_not_authorize_filtered_private_counts() {
    for kind in ["pp", "pp_max", "ep"] {
        let tree = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[{kind:{"who":"self","delta":{"count":{"who":"self","zone":"hand","name":"unit-spell"}}}}]}}),
            "unit-follower",
        );
        let failure = lowerer().lower(&tree).unwrap_err();
        assert_eq!(failure.code, "unsupported_construct");
        assert!(failure.path.starts_with("/cards/unit-follower/abilities/0"));
    }
}

#[test]
fn pp_counts_dragon_spells_instead_of_all_dragons_or_all_spells() {
    let program = program(
        &json!([{"pp":{"who":"self","delta":{"count":{"who":"self","zone":"cemetery","class":"ドラゴン","kind":"spell"}}}}]),
    );
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut other_spell = rows
        .iter()
        .find(|row| row["number"] == "unit-spell")
        .unwrap()
        .clone();
    other_spell["number"] = json!("unit-neutral-spell");
    other_spell["faces"][0]["name"] = json!("unit-neutral-spell");
    for row in &mut rows {
        if row["number"] == "unit-spell" || row["number"] == "unit-follower" {
            row["faces"][0]["card_class"] = json!("ドラゴン");
        }
    }
    rows.push(other_spell);
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-neutral-spell"] = document["cards"]["unit-spell"].clone();
    let catalogue = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    for matching in [0_u8, 2, 6] {
        let mut initial = position(1);
        let mut cemetery = vec![
            json!({"id":"dragon-follower","card":"unit-follower"}),
            json!({"id":"neutral-spell","card":"unit-neutral-spell"}),
        ];
        cemetery
            .extend((0..matching).map(|n| json!({"id":format!("spell{n}"),"card":"unit-spell"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
        let mut board = board(&program, &initial, Arc::clone(&catalogue));
        board
            .load_internal_card("unit-neutral-spell", &card(vec![]))
            .unwrap();
        let receipts = activate(&mut board, &program, 1);
        assert_eq!(receipts[0].requested, Int(i64::from(matching)));
        assert_eq!(receipts[0].actual, Int(i64::from(matching).min(4)));
    }
}

#[test]
fn pp_formula_reuses_the_paid_x_and_distinguishes_requested_from_actual() {
    let tree = equipment_played::tree(
        &json!({"activate":{"x":{},"cost":{"pp":"x"},"do":[
            {"pp":{"who":"self","delta":{"times":[2,"x"]}}}
        ]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (x, actual, final_pp) in [(0_u64, 0_i64, 4_i64), (3, 4, 5)] {
        let mut initial = position(1);
        initial["players"]["P1"]["pp"]["current"] = json!(4);
        let mut board = board(&program, &initial, catalog());
        let CardPlay::Awaiting(question) = board
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("one PP X question")
        };
        let result = resolved(
            board
                .answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                .unwrap(),
        );
        let outcome = result
            .progress
            .receipts
            .values()
            .find_map(|receipt| {
                if let InstructionOutcome::Resource(value) = receipt {
                    Some(value)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            outcome.requested,
            Int(i64::try_from(x.checked_mul(2).unwrap()).unwrap())
        );
        assert_eq!(outcome.actual, Int(actual));
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            final_pp
        );
        assert!(
            board.internal_decision(View::P1).is_none(),
            "the paid X is not selected again"
        );
    }
}

#[test]
fn author_zero_is_legal_for_each_supported_resource() {
    for kind in ["pp", "pp_max", "ep"] {
        let program = program(&json!([{kind:{"who":"self","delta":0}}]));
        let mut board = board(&program, &position(1), catalog());
        let receipts = activate(&mut board, &program, 1);
        assert_eq!(receipts[0].requested, Int(0));
        assert_eq!(receipts[0].actual, Int(0));
        assert_eq!(receipts[0].before, receipts[0].after);
    }
}

#[test]
fn resource_author_formula_overflow_restores_the_entire_activation() {
    for kind in ["pp", "pp_max", "ep"] {
        let program = program(&json!([
            {"draw":{"n":1}},
            {kind:{"who":"self","delta":{"times":[0x001F_FFFF_FFFF_FFFF_u64,{"plus":[1024,{"count":{"who":"self","zone":"hand"}}]}]}}}
        ]));
        let mut initial = position(1);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"unit-spell"}]);
        let mut board = board(&program, &initial, catalog());
        let before = exact_authority(&board);
        assert!(matches!(
            board.activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 }
            ),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&board), before);
    }
}
