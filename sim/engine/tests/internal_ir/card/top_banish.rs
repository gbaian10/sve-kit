use super::*;

fn document(cost: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{"cost":cost,"do":[{"life":{"who":"self","delta":2}}]}}),
        "unit-follower",
    )
}

fn fee() -> Value {
    json!({"steps":[{"pp":2},{"banish":{"top":{"who":"self","n":3}}}]})
}

fn position(count: usize, program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(
        1,
        json!(
            (0..count)
                .map(|n| json!({"id":format!("d{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        ),
    );
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "top-banish-payment",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn deck_top_banish_cost_is_exact_public_and_needs_no_material_choice() {
    let program = lowerer()
        .lower(&document(&fee()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for count in [3_usize, 4] {
        let mut game = position(count, &program);
        let result = resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 1_u8);
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["banish"], json!(["d0", "d1", "d2"]));
        assert_eq!(
            state["P1"]["deck"],
            if count == 3 { json!([]) } else { json!(["d3"]) }
        );
        let opponent = game.projection(View::P2).unwrap();
        for id in ["d0", "d1", "d2"] {
            assert_eq!(
                opponent["objects"][id]["name"], "unit-spell",
                "the paid cards become face-up public banished cards"
            );
        }
        assert!(
            result.progress.receipts.values().any(|outcome| {
                matches!(outcome, InstructionOutcome::Payment { quantity, subjects, .. }
                    if quantity.amount == Int(3) && quantity.unit == QuantityUnit::Cards && subjects.len() == 3)
            }),
            "the ordinary payment receipt records exactly three cards"
        );
    }
}

#[test]
fn unpayable_deck_top_banish_rolls_back_any_earlier_cost() {
    let program = lowerer()
        .lower(&document(&fee()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for count in [0_usize, 2] {
        let mut game = position(count, &program);
        let before = game.projection(View::Referee).unwrap();
        let hidden = game.projection(View::P2).unwrap();
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "insufficient cards cannot spend the earlier PP or reveal a partial slice"
        );
        assert_eq!(game.projection(View::P2).unwrap(), hidden);
        assert_eq!(game.internal_card_stage(), None);
    }
}

#[test]
fn deck_top_banish_cost_rejects_unadmitted_shapes() {
    for material in [
        json!({"top":{"who":"opp","n":3}}),
        json!({"top":{"who":"self","n":0}}),
        json!({"top":{"who":"self","n":3},"result":"paid"}),
        json!({"top":{"who":"self","n":{"count":{"who":"self","zone":"hand"}}}}),
    ] {
        lowerer()
            .lower(&document(&json!({"banish":material})))
            .unwrap_err();
    }
}

#[test]
fn deck_top_banish_cost_rejects_forged_selectors_and_quick_permissions() {
    let program = lowerer()
        .lower(&document(&fee()))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for mutation in 0..4 {
        let mut raw = program.card().clone();
        let CostSpec::Ordered { steps } = &mut raw.abilities[0].node.play.as_mut().unwrap().costs
        else {
            panic!("ordered fixture costs")
        };
        let Action::Banish {
            objects: Selector::Ordered {
                zone, start, count, ..
            },
        } = &mut steps[1].instruction.action
        else {
            panic!("fixed top-slice fixture")
        };
        match mutation {
            0 => {
                zone.player = PlayerRef::RequireOne {
                    players: Box::new(PlayerSet {
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        ..players(PlayerRelation::Opponents)
                    }),
                }
            }
            1 => zone.kind = ZoneKind::Hand,
            2 => *start = Position::Bottom {},
            _ => *count = Expr::Literal(Nat(0)),
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
    let mut quick = document(&fee());
    quick["cards"]["unit-follower"]["abilities"][0]["activate"]["quick"] = json!(true);
    let candidate = lowerer()
        .lower(&quick)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(3, &card(vec![]));
    game.load_internal_card("unit-follower", &candidate)
        .unwrap_err();
}

#[test]
fn deck_top_banish_does_not_open_optional_cost_result_declarations() {
    let mut tree = equipment_played::tree(
        &json!({"fanfare":{"optional_cost":{"pp":1,"result":"paid"},"do":[]}}),
        "unit-follower",
    );
    lowerer().lower(&tree).unwrap();
    tree["cards"]["unit-follower"]["abilities"][0]["fanfare"]["optional_cost"] =
        json!({"banish":{"top":{"who":"self","n":3}},"result":"paid"});
    lowerer().lower(&tree).unwrap_err();
}
