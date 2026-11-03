#![expect(
    clippy::panic,
    reason = "Unexpected formal choices invalidate these nested play scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree(side: &str, child: &str, extra_cost: bool, stale: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP01-041"].clone();
    let entry = template["abilities"][0].clone();
    let mut parent = entry.clone();
    let mut steps = vec![json!({"life":{"who":"self","delta":1}})];
    if stale {
        steps.push(json!({"move":{"ref":"selected","to":"ex","result":"moved"}}));
    }
    steps.extend([
        json!({"play_card":{"ref":"selected","cost":0}}),
        json!({"life":{"who":"self","delta":10}}),
    ]);
    parent["activate"] = json!({"cost":{"pp":1},"targets":[{"bind":"selected","from":{"who":side,"zone":"cemetery","name":child},"n":1}],"do":steps});
    template["abilities"] = json!([parent]);
    let mut spell = template.clone();
    let mut spell_ability = entry.clone();
    spell_ability.as_object_mut().unwrap().remove("activate");
    spell_ability["spell"] = json!({"targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],"do":[{"damage":{"to":"enemy","n":1}},{"life":{"who":"self","delta":2}}]});
    spell["abilities"] = json!([spell_ability]);
    if extra_cost {
        let mut additional = entry.clone();
        additional.as_object_mut().unwrap().remove("activate");
        additional["play"] = json!({"additional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":1}}});
        spell["abilities"].as_array_mut().unwrap().push(additional);
    }
    let mut follower = template.clone();
    let mut fanfare = entry;
    fanfare.as_object_mut().unwrap().remove("activate");
    fanfare["fanfare"] = json!({"do":[{"if":{"cond":{"entered_by":{"ref":"this","by":"play"}},"then":[{"life":{"who":"self","delta":5}}]}}]});
    follower["abilities"] = json!([fanfare]);
    tree["cards"] = json!({"unit-amulet":template,"unit-spell":spell,"unit-follower":follower});
    tree
}
#[derive(Clone, Copy)]
enum Scenario {
    Plain,
    AdditionalCost { material: bool },
    MandatoryReveal { material: bool },
    StaleSelection,
    FullField,
    TwoLayers,
    ParentDeparture,
}

#[expect(
    clippy::panic,
    reason = "This fixture probes the lowered parent life instruction."
)]
fn use_parent_source_cost(programs: &mut BTreeMap<String, WellFormedCardProgram>) {
    let mut parent = programs["unit-amulet"].card().clone();
    let Node::Instruction { instruction } = &mut parent.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .last_mut()
        .unwrap()
        .node
    else {
        panic!("parent final instruction");
    };
    let Action::Life {
        change: NumericChange::Add { amount },
        ..
    } = &mut instruction.action
    else {
        panic!("parent final life change");
    };
    *amount = source_information::source_read(
        &ObjectView::SourceInformation {},
        CharacteristicView::Effective,
        None,
    );
    programs.insert(
        "unit-amulet".into(),
        WellFormedCardProgram::check(parent).unwrap(),
    );
}

fn build(side: &str, child: &str, enemy: bool, scenario: Scenario) -> (Game, AbilityId) {
    let extra_cost = matches!(
        scenario,
        Scenario::AdditionalCost { .. } | Scenario::MandatoryReveal { .. }
    );
    let stale = matches!(scenario, Scenario::StaleSelection);
    let mut input = tree(side, child, extra_cost, stale);
    if matches!(scenario, Scenario::MandatoryReveal { .. }) {
        input["cards"]["unit-spell"]["abilities"][1]["play"]["additional_cost"] =
            json!({"reveal":{"sel":{"who":"self","zone":"hand","kind":"follower"},"n":1}});
        input["cards"]["unit-spell"]["abilities"][0]["spell"]["do"].as_array_mut().unwrap().insert(1,
            json!({"choose":{"bind":"pause","from":{"who":"self","zone":"hand","kind":"follower"},"n":1}}));
    }
    if matches!(scenario, Scenario::TwoLayers) {
        let mut leaf = input["cards"]["unit-spell"].clone();
        leaf["abilities"][0]["spell"] = json!({"do":[{"life":{"who":"self","delta":3}}]});
        input["cards"]["unit-leaf"] = leaf;
        input["cards"]["unit-spell"]["abilities"][0]["spell"] = json!({"targets":[{"bind":"leaf","from":{"who":"self","zone":"cemetery","name":"unit-leaf"},"n":1}],"do":[{"play_card":{"ref":"leaf","cost":0}},{"life":{"who":"self","delta":2}}]});
    }
    if matches!(scenario, Scenario::ParentDeparture) {
        input["cards"]["unit-spell"]["abilities"][0]["spell"] = json!({"targets":[{"bind":"parent","from":{"who":"self","zone":"field","kind":"amulet"},"n":1}],"do":[{"destroy":"parent"},{"life":{"who":"self","delta":2}}]});
    }
    let mut programs = lowerer()
        .lower_with_printed_costs(
            &input,
            &BTreeMap::from([("unit-spell".into(), Nat(1)), ("unit-leaf".into(), Nat(1))]),
        )
        .unwrap();
    if matches!(scenario, Scenario::ParentDeparture) {
        use_parent_source_cost(&mut programs);
    }
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    if matches!(scenario, Scenario::MandatoryReveal { material: true }) {
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"hidden","card":"unit-follower"}]);
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"parent","card":"unit-amulet"}]);
    if matches!(scenario, Scenario::AdditionalCost { material: true }) {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"unit-follower"}));
    }
    initial["players"]["P2"]["zones"]["field"] = if enemy {
        json!([{"id":"enemy","card":"unit-follower"}])
    } else {
        json!([])
    };
    if matches!(scenario, Scenario::FullField) {
        for slot in 0..4 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("filler{slot}"),"card":"unit-follower"}));
        }
    }
    let owner = if side == "self" { "P1" } else { "P2" };
    initial["players"][owner]["zones"]["cemetery"] = json!([{"id":"child","card":child}]);
    let catalog = if matches!(scenario, Scenario::TwoLayers) {
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"leaf","card":"unit-leaf"}));
        let (mut snapshot, keywords, document) = catalog_documents(&json!("1"));
        let mut leaf = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["number"] == "unit-spell")
            .unwrap();
        leaf["number"] = json!("unit-leaf");
        leaf["faces"][0]["name"] = json!("unit-leaf");
        snapshot.push('\n');
        snapshot.push_str(&leaf.to_string());
        let extra = json!({"format":1,"kind":"effect_set","cards":{"unit-leaf":{"status":"complete","abilities":[]}}}).to_string();
        Arc::new(
            Catalog::from_documents(
                &snapshot,
                &keywords,
                &[("unit.yaml".into(), document), ("leaf.yaml".into(), extra)],
            )
            .unwrap(),
        )
    } else {
        catalog()
    };
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "nested-card").unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    (game, programs["unit-amulet"].card().abilities[0].node.id)
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target question");
    };
    assert!(
        matches!(question.domain, InputDomain::Targets(_)),
        "expected a formal target domain"
    );
    question
}
fn select(game: &mut Game, question: &DecisionView) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap()
}

#[test]
fn a_cemetery_spell_declares_its_own_targets_and_resumes_the_parent_after_cleanup() {
    for side in ["self", "opp"] {
        let (mut game, definition) = build(side, "unit-spell", true, Scenario::Plain);
        let outer = question(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        let inner = question(select(&mut game, &outer));
        assert_ne!(outer.token.execution, inner.token.execution);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["leader"]["life"], 21_u8);
        assert_eq!(before["P1"]["pp"]["current"], 2_u8);
        game.answer_internal_card(outer.actor, outer.token, Answer::Targets(vec![0]))
            .unwrap_err();
        assert_eq!(game.internal_decision(View::P1).unwrap(), inner);
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let result = resolved(select(&mut game, &inner));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(result.source.definition, definition);
        assert_eq!(result.execution, outer.token.execution);
        assert_eq!(state["P1"]["leader"]["life"], 33_u8);
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
        let owner = if side == "self" { "P1" } else { "P2" };
        assert_eq!(state[owner]["cemetery"], json!(["child"]));
        assert_eq!(
            state["semantic_state"]["counters_this_turn"]["P1"]["cards_played"],
            1_u8
        );
        let child_resolved = result.progress.facts.iter().position(|fact| matches!(fact,Fact::CardResolved{execution,..} if *execution==inner.token.execution)).unwrap();
        let parent_resolved = result.progress.facts.iter().position(|fact| matches!(fact,Fact::AbilityResolved{execution,..} if *execution==outer.token.execution)).unwrap();
        assert!(child_resolved < parent_resolved);
    }
}
#[test]
fn missing_child_targets_keep_the_parent_payment_and_earlier_body() {
    let (mut game, definition) = build("self", "unit-spell", false, Scenario::Plain);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let result = resolved(select(&mut game, &outer));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 31_u8);
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["child"]));
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
}
#[test]
fn zero_base_override_preserves_required_material_costs_and_child_only_failure() {
    for material in [false, true] {
        let (mut game, definition) = build(
            "self",
            "unit-spell",
            true,
            Scenario::AdditionalCost { material },
        );
        let outer = question(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        let child = select(&mut game, &outer);
        let result = if material {
            let CardPlay::Awaiting(materials) = child else {
                panic!("expected the required additional material question");
            };
            assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
            let victim = question(
                game.answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
            );
            resolved(select(&mut game, &victim))
        } else {
            resolved(child)
        };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if material { 33_u8 } else { 31_u8 }
        );
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
        assert_eq!(
            state["P1"]["cemetery"],
            if material {
                json!(["material", "child"])
            } else {
                json!(["child"])
            }
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                .count(),
            usize::from(material)
        );
    }
}
#[test]
fn a_selected_old_generation_cannot_play_the_card_after_an_earlier_parent_movement() {
    let (mut game, definition) = build("self", "unit-spell", true, Scenario::StaleSelection);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let result = resolved(select(&mut game, &outer));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 31_u8);
    assert_eq!(state["P1"]["ex"], json!(["child"]));
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
}
#[test]
fn a_nested_follower_enters_but_its_fanfare_waits_for_the_parent_to_finish() {
    let (mut game, definition) = build("opp", "unit-follower", false, Scenario::Plain);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let result = resolved(select(&mut game, &outer));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 31_u8);
    assert_eq!(state["P1"]["field"], json!(["parent", "child"]));
    assert!(result.progress.facts.iter().any(
        |fact| matches!(fact,Fact::CardPlayed{execution,..} if *execution!=outer.token.execution)
    ));
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the deferred fanfare question");
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        36_u8
    );
}
#[test]
fn effect_play_keeps_unrequested_author_shapes_closed() {
    let baseline = tree("self", "unit-spell", false, false);
    for shape in [
        json!({"ref":"selected"}),
        json!({"ref":"selected","cost":1}),
        json!({"ref":"this","cost":0}),
        json!({"ref":"selected","result":"played"}),
        json!({"ref":"selected","cost_policy":"waive_original"}),
    ] {
        let mut input = baseline.clone();
        input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][1]["play_card"] = shape;
        lowerer()
            .lower_with_printed_costs(&input, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
            .unwrap_err();
    }
}

#[test]
fn full_field_cancels_the_child_entry_without_undoing_the_parent() {
    let (mut game, definition) = build("opp", "unit-follower", false, Scenario::FullField);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let result = resolved(select(&mut game, &outer));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P2"]["cemetery"], json!(["child"]));
    assert_eq!(state["P1"]["leader"]["life"], 31_u8);
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
}

#[test]
fn two_child_layers_keep_distinct_executions_and_resume_both_parent_bodies() {
    let (mut game, definition) = build("self", "unit-spell", false, Scenario::TwoLayers);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let inner = question(select(&mut game, &outer));
    let result = resolved(select(&mut game, &inner));
    assert_eq!(result.execution, outer.token.execution);
    assert_eq!(result.source.definition, definition);
    let children = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::CardPlayed { execution, .. } = fact {
                Some(*execution)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(children.len(), 2);
    assert_ne!(children[0], children[1]);
    assert!(children.iter().all(|id| *id != outer.token.execution));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 36_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["leaf", "child"]));
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
}

#[test]
fn a_child_departure_seals_the_suspended_parents_source_information() {
    let (mut game, definition) = build("self", "unit-spell", false, Scenario::ParentDeparture);
    let outer = question(
        game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let inner = question(select(&mut game, &outer));
    let result = resolved(select(&mut game, &inner));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(result.execution, outer.token.execution);
    assert_eq!(result.source.definition, definition);
    assert_eq!(state["P1"]["leader"]["life"], 24_u8);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["parent", "child"]));
}

#[test]
fn nested_spell_revelation_keeps_private_payment_and_resumes_the_parent() {
    for (side, material) in [
        ("self", false),
        ("self", true),
        ("opp", false),
        ("opp", true),
    ] {
        let (mut game, definition) = build(
            side,
            "unit-spell",
            true,
            Scenario::MandatoryReveal { material },
        );
        let outer = question(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        let child = select(&mut game, &outer);
        let result = if material {
            let CardPlay::Awaiting(materials) = child else {
                panic!("mandatory revelation materials");
            };
            assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
            assert!(game.internal_decision(View::P2).is_none());
            assert!(game.projection(View::P2).unwrap()["objects"]["hidden"].is_null());
            let target = question(
                game.answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
            );
            let body = question(select(&mut game, &target));
            assert_eq!(body.token.execution, materials.token.execution);
            assert_ne!(body.token.execution, outer.token.execution);
            assert!(game.internal_decision(View::P2).is_none());
            assert_eq!(
                game.projection(View::P2).unwrap()["objects"]["hidden"]["card"],
                "unit-follower"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                21_u8
            );
            resolved(select(&mut game, &body))
        } else {
            resolved(child)
        };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if material { 33_u8 } else { 31_u8 }
        );
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
        let owner = if side == "self" { "P1" } else { "P2" };
        assert_eq!(state[owner]["cemetery"], json!(["child"]));
        assert_eq!(
            state["P1"]["hand"],
            if material {
                json!(["hidden"])
            } else {
                json!([])
            }
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["hidden"].is_null());
        assert_eq!(result.source.definition, definition);
        assert_eq!(result.execution, outer.token.execution);
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
    }
}
