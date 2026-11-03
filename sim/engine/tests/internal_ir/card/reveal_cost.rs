use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree(cost: &Value, steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({"cost":cost,"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    tree
}

fn program(cost: &Value, steps: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(cost, steps))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn cost(n: u64, later_failure: bool) -> Value {
    let mut steps = vec![
        json!({"pp":1}),
        json!({"act_self":true}),
        json!({"reveal":{"sel":{"who":"self","zone":"hand","kind":"spell","orig_cost_eq":1},"n":n}}),
    ];
    if later_failure {
        steps.push(json!({"pp":99}));
    }
    json!({"steps":steps})
}

fn position(seat: u32, acted: bool, count: u64, program: &WellFormedCardProgram) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":5,"max":5});
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":acted}}]);
    let mut hand = (0..count)
        .map(|n| json!({"id":format!("h{n}"),"card":"unit-spell","state":{"cost":9}}))
        .collect::<Vec<_>>();
    hand.push(json!({"id":"wrong","card":"unit-follower"}));
    initial["players"][actor]["zones"]["hand"] = json!(hand);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "reveal-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn reject_material_answers(game: &mut Game, question: &DecisionView, seat: u32) {
    let pending = exact_authority(game);
    for handles in [vec![], vec![0], vec![0, 0], vec![0, 99], vec![0, 1, 2]] {
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(handles),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(game),
            pending,
            "invalid answer must preserve the pending question"
        );
    }
    game.answer_internal_card(
        PlayerId {
            seat: if seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::CostMaterials(vec![0, 1]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(game),
        pending,
        "invalid answer must preserve the pending question"
    );
    let mut stale = question.token;
    stale.epoch = stale.epoch.checked_add(1).unwrap();
    game.answer_internal_card(question.actor, stale, Answer::CostMaterials(vec![0, 1]))
        .unwrap_err();
    assert_eq!(
        exact_authority(game),
        pending,
        "invalid answer must preserve the pending question"
    );
}

#[test]
fn fixed_reveal_cost_uses_private_scoped_handles_and_only_reveals_paid_materials() {
    let program = program(
        &cost(2, false),
        &json!([
            {"choose":{"bind":"later","from":{"who":"self","zone":"hand"},"n":1}},
            {"move":{"ref":"later","to":"deck","position":"bottom"}}
        ]),
    );
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let other = if seat == 1 { View::P2 } else { View::P1 };
        let mut game = position(seat, false, 3, &program);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("material choice");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("materials");
        };
        assert_eq!(domain.members.len(), 3);
        assert_eq!(domain.cardinality.minimum, Nat(2));
        assert_ne!(domain.members[0].handle, domain.members[1].handle);
        assert!(game.internal_decision(other).is_none());
        assert!(game.projection(other).unwrap()["objects"]["h0"].is_null());
        reject_material_answers(&mut game, &question, seat);
        let CardPlay::Awaiting(later) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0, 1]),
            )
            .unwrap()
        else {
            panic!("body choice");
        };
        let during = game.projection(other).unwrap();
        assert_eq!(during["objects"]["h0"]["card"], "unit-spell");
        assert_eq!(during["objects"]["h1"]["card"], "unit-spell");
        assert!(during["objects"]["h2"].is_null());
        assert!(during["objects"]["wrong"].is_null());
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["pp"]["current"],
            4
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
            true
        );
        assert!(game.internal_decision(other).is_none());
        let result = resolved(
            game.answer_internal_card(later.actor, later.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["cemetery"],
            json!([])
        );
        let after = game.projection(other).unwrap();
        for id in ["h0", "h1", "h2", "wrong"] {
            assert!(after["objects"][id].is_null());
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()[actor]["deck"],
            json!(["h0"])
        );
    }
}

#[test]
fn reveal_cost_is_atomic_with_source_act_and_a_later_failed_payment() {
    for (acted, count, failure) in [(false, 1, false), (true, 2, false), (false, 2, true)] {
        let program = program(&cost(2, failure), &json!([]));
        let mut game = position(1, acted, count, &program);
        let before = game.projection(View::Referee).unwrap();
        let start = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if count < 2 {
            assert!(matches!(
                start.unwrap_err(),
                sve_engine::EngineFailure::IllegalPlay(_)
            ));
        } else {
            let CardPlay::Awaiting(question) = start.unwrap() else {
                panic!("materials");
            };
            let error = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![0, 1]),
                )
                .unwrap_err();
            assert!(matches!(error, sve_engine::EngineFailure::IllegalPlay(_)));
        }
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        assert!(game.internal_decision(View::Referee).is_none());
        assert!(game.projection(View::P2).unwrap()["objects"]["h0"].is_null());
    }
}

#[test]
fn reveal_materials_do_not_authorize_other_hand_costs_or_public_formal_targets() {
    let original = program(&cost(1, false), &json!([]));
    for replacement in [
        "banish",
        "move",
        "act",
        "target",
        "public",
        "opponent",
        "unconsumed",
    ] {
        let mut program = original.card().clone();
        let play = program.abilities[0].node.play.as_mut().unwrap();
        if replacement == "unconsumed" {
            let CostSpec::Ordered { steps } = &mut play.costs else {
                panic!("cost");
            };
            steps.pop();
        } else if matches!(replacement, "banish" | "move" | "act") {
            let CostSpec::Ordered { steps } = &mut play.costs else {
                panic!("cost");
            };
            let Action::Reveal { objects } = &steps[2].instruction.action else {
                panic!("reveal");
            };
            steps[2].instruction.action = match replacement {
                "banish" => Action::Banish {
                    objects: objects.clone(),
                },
                "act" => Action::Posture {
                    objects: objects.clone(),
                    posture: Posture::Acted,
                },
                _ => Action::Move {
                    objects: objects.clone(),
                    movement: MoveSpec {
                        to: MoveDestination::Owners {
                            zone: OwnerZone::Hand,
                        },
                        position: None,
                        order: None,
                        initial_posture: None,
                        entry_effects: vec![],
                    },
                    reason: MoveReason::Put,
                },
            };
        } else {
            let decision = &mut play.parameters[0];
            match replacement {
                "target" => {
                    if let DecisionKind::PlayParameter { slot, .. } = &mut decision.kind {
                        *slot = PlaySlot::Targets;
                    }
                }
                "public" => decision.visibility = Visibility::Public {},
                _ => {
                    if let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain
                        && let Selector::Zone { players, .. } = &mut constraint.groups[0].selector
                    {
                        players.relation = PlayerRelation::Opponents;
                    }
                }
            }
        }
        let Ok(program) = WellFormedCardProgram::check(program) else {
            continue;
        };
        let mut game = game(1, 0);
        let before = exact_authority(&game);
        game.load_internal_card("unit-follower", &program)
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn reveal_cost_lowering_rejects_unrequested_material_shapes() {
    let mut mixed = cost(1, false);
    mixed["steps"].as_array_mut().unwrap().push(json!({
        "discard":{"sel":{"who":"self","zone":"hand"},"n":1}
    }));
    lowerer().lower(&tree(&mixed, &json!([]))).unwrap_err();
    lowerer()
        .lower(&tree(
            &json!({"discard":{
                "sel":{"who":"self","zone":"hand","name_contains":"オーバーロード"},"n":1
            }}),
            &json!([]),
        ))
        .unwrap_err();
    let mut source_move = cost(1, false);
    source_move["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"self_to_cemetery":true}));
    lowerer()
        .lower(&tree(&source_move, &json!([])))
        .unwrap_err();
    for material in [
        json!({"sel":{"who":"opp","zone":"hand"},"n":1}),
        json!({"sel":{"who":"self","zone":"cemetery"},"n":1}),
        json!({"sel":{"who":"self","zone":"hand"},"n":0}),
        json!({"sel":{"who":"self","zone":"hand"},"n":3}),
        json!({"sel":{"who":"self","zone":"hand"},"upto":1}),
        json!({"sel":{"who":"self","zone":"hand"},"any":true}),
        json!({"sel":{"who":"self","zone":"hand","name_contains":""},"n":1}),
        json!({"sel":{"who":"self","zone":"hand"},"n":1,"result":"paid"}),
    ] {
        lowerer()
            .lower(&tree(&json!({"reveal":material}), &json!([])))
            .unwrap_err();
    }
}

#[test]
fn reveal_cost_filters_original_names_kinds_traits_and_costs() {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            let follower = card["number"] == "unit-follower";
            let face = &mut card["faces"][0];
            face["name"] = json!(if follower {
                "オーバーロード・候補"
            } else {
                "オーバーロード・非从者"
            });
            face["traits"] = json!(["学院"]);
            if !follower {
                face["cost"] = json!("2");
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    for (filter, expected) in [
        (
            json!({"kind":"follower","name_contains":"オーバーロード"}),
            vec!["unit-follower"],
        ),
        (
            json!({"kind":"follower","trait":"学院"}),
            vec!["unit-follower"],
        ),
        (json!({"orig_cost_eq":2}), vec!["unit-spell", "unit-amulet"]),
    ] {
        let mut query = filter;
        query["who"] = json!("self");
        query["zone"] = json!("hand");
        let program = program(&json!({"reveal":{"sel":query,"n":1}}), &json!([]));
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"follower","card":"unit-follower"},
            {"id":"spell","card":"unit-spell","state":{"cost":0}},
            {"id":"amulet","card":"unit-amulet","state":{"cost":0}}
        ]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "reveal-filter",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        for number in ["unit-spell", "unit-amulet"] {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("materials");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("materials");
        };
        assert_eq!(
            domain
                .members
                .iter()
                .map(|member| member.card.as_deref().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["follower", "spell", "amulet"])
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["spell"].is_null());
    }
}

#[test]
fn required_reveal_head_cancels_without_revealing_when_declined_or_unpayable() {
    let mut expanded = tree(&cost(1, false), &json!([]));
    let ability = &mut expanded["cards"]["unit-follower"]["abilities"][0];
    ability.as_object_mut().unwrap().remove("activate");
    ability["fanfare"] = json!({"cost":{"reveal":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}},"do":[{"draw":{"n":1}}]});
    let program = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (accept, eligible) in [(true, true), (false, true), (true, false)] {
        let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"s","card":"unit-follower"},
            {"id":"h","card":if eligible {"unit-spell"} else {"unit-follower"}}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "reveal-head",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
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
            panic!("declaration");
        };
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let play = game
            .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accept))
            .unwrap();
        if accept && eligible {
            let CardPlay::Awaiting(materials) = play else {
                panic!("materials");
            };
            assert!(game.internal_decision(View::P2).is_none());
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
                json!(["h", "drawn"])
            );
        } else {
            assert!(matches!(play, CardPlay::Cancelled));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["h"])
            );
        }
        assert!(game.projection(View::P2).unwrap()["objects"]["h"].is_null());
    }
}

#[test]
fn no_legal_formal_target_prevents_reveal_materials_and_source_payment() {
    let mut expanded = tree(&cost(2, false), &json!([{"damage":{"to":"enemy","n":1}}]));
    expanded["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] = json!([
        {"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
    ]);
    let program = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"h0","card":"unit-spell"},{"id":"h1","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "no-reveal-target",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert!(game.internal_decision(View::Referee).is_none());
    assert!(game.projection(View::P2).unwrap()["objects"]["h0"].is_null());
}

#[test]
fn paid_reveal_material_bindings_are_unavailable_to_the_ability_body() {
    for head in [false, true] {
        let mut expanded = tree(&cost(1, false), &json!([]));
        if head {
            let entry = &mut expanded["cards"]["unit-follower"]["abilities"][0];
            entry.as_object_mut().unwrap().remove("activate");
            entry["fanfare"] =
                json!({"cost":{"reveal":{"sel":{"who":"self","zone":"hand"},"n":1}},"do":[]});
        }
        let mut program = lowerer()
            .lower(&expanded)
            .unwrap()
            .remove("unit-follower")
            .unwrap()
            .card()
            .clone();
        let ability = &mut program.abilities[0];
        let ChoiceDomain::Objects { binding, .. } =
            ability.node.play.as_ref().unwrap().parameters[0].domain
        else {
            panic!("private reveal material binding");
        };
        let mut leak = instruction(
            999,
            Action::Reveal {
                objects: Selector::Bound { reference: binding },
            },
        );
        let Node::Instruction { instruction } = &mut leak.node else {
            panic!("reveal instruction");
        };
        instruction.source = ability.node.source.clone();
        leak.source = ability.source.clone();
        ability.node.body.as_mut().unwrap().steps.push(leak);
        let error = WellFormedCardProgram::check(program).unwrap_err();
        assert_eq!(error.reason, WellFormedReason::UnavailableBinding);
    }
}
