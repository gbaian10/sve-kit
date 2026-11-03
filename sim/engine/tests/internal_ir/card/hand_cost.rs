use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

fn program(later_failure: bool) -> WellFormedCardProgram {
    filtered_program(
        &json!({"who":"self","zone":"hand","kind":"spell"}),
        later_failure,
    )
}

fn filtered_program(query: &Value, later_failure: bool) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    let mut costs = vec![
        json!({"pp":1}),
        json!({"act_self":true}),
        json!({"discard":{"sel":query,"n":1}}),
    ];
    if later_failure {
        costs.push(json!({"pp":99}));
    }
    ability["activate"] = json!({"cost":{"steps":costs},"do":[{"draw":{"n":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn hand_cost_selection_is_private_strict_and_payment_rolls_back_as_a_whole() {
    for (has_spell, acted, later_failure) in [
        (true, false, false),
        (false, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"acted":acted}}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":if has_spell {"unit-spell"} else {"unit-follower"}}, {"id":"other","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "hand-cost").unwrap();
        game.load_internal_card("unit-follower", &program(later_failure))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let before = game.projection(View::Referee).unwrap();
        let start = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if !has_spell {
            start.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        let CardPlay::Awaiting(question) = start.unwrap() else {
            panic!("hand materials");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("cost materials");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2
        );
        assert!(game.internal_decision(View::P2).is_none());
        let pending = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), pending);
        for handles in [vec![], vec![0, 0], vec![99]] {
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(handles),
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), pending);
        }
        let result = game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        );
        if acted || later_failure {
            result.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            assert!(game.internal_decision(View::Referee).is_none());
        } else {
            let result = resolved(result.unwrap());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["cemetery"], json!(["h"]));
            assert_eq!(view["P1"]["hand"], json!(["other", "d"]));
            assert_eq!(view["P1"]["pp"]["current"], 1);
            assert_eq!(view["objects"]["s"]["acted"], true);
            let discard = result
                .progress
                .facts
                .iter()
                .position(|fact| matches!(fact, Fact::Discard(_)))
                .unwrap();
            let played = result
                .progress
                .facts
                .iter()
                .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .unwrap();
            assert!(discard < played);
        }
    }
}

#[test]
fn filtered_hand_cost_uses_class_cost_and_all_traits_without_leaking_other_candidates() {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut face: Value = serde_json::from_str(line).unwrap();
            if face["number"] == "unit-spell" {
                face["faces"][0]["cost"] = json!("7");
                face["faces"][0]["card_class"] = json!("ドラゴン");
                face["faces"][0]["traits"] = json!(["絶傑", "魔法使い"]);
            }
            if face["number"] == "unit-follower" {
                face["faces"][0]["traits"] = json!(["魔法使い"]);
            }
            if face["number"] == "unit-amulet" {
                face["faces"][0]["cost"] = json!("7");
                face["faces"][0]["traits"] = json!(["絶傑"]);
            }
            face.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("filter.yaml".into(), document)]).unwrap(),
    );
    for (filter, expected) in [
        (
            json!({"class":"ドラゴン","orig_cost_ge":7}),
            vec!["unit-spell"],
        ),
        (json!({"orig_cost_eq":7}), vec!["unit-spell", "unit-amulet"]),
        (json!({"orig_cost_le":1}), vec!["unit-follower"]),
        (json!({"class":"ロイヤル","orig_cost_ge":7}), vec![]),
        (
            json!({"traits_all":["絶傑","魔法使い"]}),
            vec!["unit-spell"],
        ),
        (json!({"traits_all":["絶傑","missing"]}), vec![]),
    ] {
        let mut query = filter;
        query["who"] = json!("self");
        query["zone"] = json!("hand");
        let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"seven","card":"unit-spell","state":{"cost":0}},
            {"id":"low","card":"unit-follower","state":{"cost":9}},
            {"id":"wrong-class","card":"unit-amulet"}]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "filtered",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &filtered_program(&query, false))
            .unwrap();
        for name in ["unit-spell", "unit-amulet"] {
            game.load_internal_card(name, &card(vec![])).unwrap();
        }
        let before = game.projection(View::Referee).unwrap();
        let start = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if expected.is_empty() {
            start.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        let CardPlay::Awaiting(question) = start.unwrap() else {
            panic!("private materials");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("hand cost");
        };
        assert_eq!(
            domain
                .members
                .iter()
                .map(|member| member.card.as_deref().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(game.internal_decision(View::P2).is_none());
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn filtered_hand_cost_rejects_numeric_reads_outside_its_private_candidate() {
    let mut program =
        filtered_program(&json!({"who":"self","zone":"hand","orig_cost_ge":7}), false)
            .card()
            .clone();
    let CostSpec::Ordered { steps } = &mut program.abilities[0].node.play.as_mut().unwrap().costs
    else {
        panic!("synthetic cost shape")
    };
    let Action::Discard {
        mode: DiscardMode::Chosen { constraint },
        ..
    } = &mut steps[2].instruction.action
    else {
        panic!("synthetic cost shape")
    };
    let Selector::Zone { predicate, .. } = &mut constraint.groups[0].selector else {
        panic!("synthetic cost shape")
    };
    **predicate = Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Cost {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                view: CharacteristicView::Original,
            },
        })),
        right: Expr::Literal(Int(7)),
        comparison: Comparison::GreaterEqual,
    }));
    let program = WellFormedCardProgram::check(program).unwrap();
    let mut game = game(1, 0);
    let before = exact_authority(&game);
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

fn head_program() -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability["fanfare"] = json!({"cost":{"discard":{"sel":{"who":"self","zone":"hand","kind":"spell","class":"ニュートラル","orig_cost_eq":1},"n":1}},"do":[{"draw":{"n":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn filtered_trigger_head_declares_before_private_materials_and_can_decline_or_fail() {
    for (accept, eligible) in [(true, true), (false, true), (true, false)] {
        let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"},
            {"id":"h","card":if eligible {"unit-spell"} else {"unit-follower"}}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "head-filter",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &head_program())
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let CardPlay::Awaiting(declare) = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap()
        else {
            panic!("head declaration");
        };
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let before = game.projection(View::Referee).unwrap();
        let play =
            game.answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accept));
        if !eligible {
            assert!(matches!(play.unwrap(), CardPlay::Cancelled));
            let cancelled = game.projection(View::Referee).unwrap();
            assert_eq!(cancelled["P1"], before["P1"]);
            assert_eq!(cancelled["objects"], before["objects"]);
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            assert!(game.internal_pending_choice().unwrap().is_none());
        } else if !accept {
            assert!(matches!(play.unwrap(), CardPlay::Cancelled));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["h"])
            );
        } else {
            let CardPlay::Awaiting(materials) = play.unwrap() else {
                panic!("private hand materials");
            };
            assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
            assert!(game.internal_decision(View::P2).is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["h"])
            );
            resolved(
                game.answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["drawn"])
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!(["h"])
            );
        }
    }
}

fn grouped_tree(reverse: bool, later_failure: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    let mut groups = vec![
        json!({"sel":{"in":"this"},"n":1_u8}),
        json!({"sel":{"who":"self","zone":"hand"},"n":1_u8}),
    ];
    if reverse {
        groups.reverse();
    }
    let mut costs = vec![json!({"pp":1_u8}), json!({"discard":{"groups":groups}})];
    if later_failure {
        costs.push(json!({"pp":99_u8}));
    }
    ability["activate"] = json!({"zone":"hand","cost":{"steps":costs},"do":[{"draw":{"n":1_u8}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    tree
}

#[test]
fn grouped_hand_cost_reserves_its_source_and_rolls_back_both_materials_atomically() {
    for reverse in [false, true] {
        for later_failure in [false, true] {
            for has_other in [false, true] {
                let program = lowerer()
                    .lower(&grouped_tree(reverse, later_failure))
                    .unwrap()
                    .remove("unit-follower")
                    .unwrap();
                let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
                initial["players"]["P1"]["zones"]["field"] = json!([]);
                initial["players"]["P1"]["zones"]["hand"] =
                    json!([{"id":"source","card":"unit-follower"}]);
                if has_other {
                    initial["players"]["P1"]["zones"]["hand"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"other","card":"unit-follower"}));
                }
                let mut board = Game::new(
                    catalog(),
                    &initial,
                    &Value::Null,
                    &Value::Null,
                    "grouped-hand-cost",
                )
                .unwrap();
                board.load_internal_card("unit-follower", &program).unwrap();
                board
                    .load_internal_card("unit-spell", &card(vec![]))
                    .unwrap();
                let before = board.projection(View::Referee).unwrap();
                let start =
                    board.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 });
                if !has_other {
                    start.unwrap_err();
                    assert_eq!(board.projection(View::Referee).unwrap(), before);
                    continue;
                }
                let CardPlay::Awaiting(question) = start.unwrap() else {
                    panic!("one other material");
                };
                let InputDomain::CostMaterials(domain) = &question.domain else {
                    panic!("private grouped cost");
                };
                assert_eq!(
                    domain.members.len(),
                    1,
                    "source itself is reserved, even though both cards match"
                );
                assert_eq!(
                    domain.cardinality,
                    Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1))
                    }
                );
                assert!(board.internal_decision(View::P2).is_none());
                let result = board.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                );
                if later_failure {
                    result.unwrap_err();
                    assert_eq!(
                        board.projection(View::Referee).unwrap(),
                        before,
                        "a later failure restores PP, both cards and pending occurrences"
                    );
                } else {
                    resolved(result.unwrap());
                    let state = board.projection(View::Referee).unwrap();
                    assert_eq!(state["P1"]["hand"], json!(["d"]));
                    assert_eq!(state["P1"]["cemetery"], json!(["source", "other"]));
                    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
                }
            }
        }
    }
}

#[test]
fn grouped_hand_cost_keeps_unneeded_material_shapes_closed() {
    let source = json!({"sel":{"in":"this"},"n":1_u8});
    let other = json!({"sel":{"who":"self","zone":"hand"},"n":1_u8});
    for groups in [
        json!([source.clone(), source.clone()]),
        json!([other.clone(), other.clone()]),
        json!([source.clone(), other.clone(), other.clone()]),
        json!([{"sel":{"in":"this"},"n":2_u8}, other]),
        json!([source.clone(), {"sel":{"who":"self","zone":"hand"},"n":2_u8}]),
        json!([source.clone(), {"sel":{"who":"self","zone":"hand"},"upto":1_u8}]),
        json!([source, {"sel":{"who":"self","zone":"hand"},"n":1_u8,"result":"paid"}]),
    ] {
        let mut tree = grouped_tree(false, false);
        tree["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"][1]["discard"]
            ["groups"] = groups;
        lowerer().lower(&tree).unwrap_err();
    }
    let mut tree = grouped_tree(false, false);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"discard":{"ref":"this"}}));
    lowerer().lower(&tree).unwrap_err();
}
