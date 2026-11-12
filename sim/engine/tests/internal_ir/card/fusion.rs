#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these payment and event scenarios."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn position(up_to: bool, full_ex: bool) -> (Game, AbilityId) {
    let mut fusion =
        json!({"sel":{"who":"self","zone":["hand","ex"],"kind":"spell","orig_cost_ge":1}});
    fusion[if up_to { "upto" } else { "n" }] = json!(if up_to { 2_u8 } else { 1_u8 });
    fusion["result"] = json!("materials");
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":fusion,
            "do":[{"if":{"cond":{"cmp":{
                "left":{"count":{"in":{"objects_of":"materials","view":"before"},"kind":"spell"}},
                "op":"eq","right":if up_to { 2_u8 } else { 1_u8 }
            }},"then":[{"counter":{"on":"this","name":"fusion_counter","delta":1_i8}}]}}]
        }),
    )]);
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"unit-follower"}, {"id":"h","card":"unit-spell"},
        {"id":"wrong","card":"unit-amulet"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
    if full_ex {
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"e","card":"unit-spell"}, {"id":"e1","card":"unit-amulet"},
            {"id":"e2","card":"unit-amulet"}, {"id":"e3","card":"unit-amulet"},
            {"id":"e4","card":"unit-amulet"}
        ]);
    }
    let mut game = evolution_linked_movement::loaded(&initial, program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    (game, program.card().abilities[0].node.id)
}

#[test]
fn mixed_fusion_discards_only_hand_material_and_follows_source_into_ex() {
    let (mut game, ability) = position(true, true);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Fusion materials");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("materials domain")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only matching own hand and EX materials"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "Fusion cannot choose zero while material exists"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "hand candidates stay private"
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "another player cannot pay the private cost"
    );
    for handles in [vec![], vec![0, 0], vec![99]] {
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(handles),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "bad answer preserves the pending payment"
        );
    }
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1]),
        )
        .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Discard(_)))
            .count(),
        1,
        "EX material is Put, not Discard"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"].as_array().unwrap().len(),
        5,
        "EX material frees the source slot"
    );
    assert_eq!(state["P1"]["cemetery"], json!(["h", "e"]));
    assert_eq!(
        state["objects"]["source"]["counters"]["fusion_counter"], 1_u8,
        "body sees both paid spell materials and the source's new EX generation"
    );
    let after = exact_authority(&game);
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        after,
        "a stale payment answer cannot be replayed"
    );
}

#[test]
fn zero_material_fusion_moves_the_source_but_skips_the_entire_body() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"upto":2_u8},
            "do":[{"counter":{"on":"this","name":"fusion_counter","delta":1_i8}}]
        }),
    )]);
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = evolution_linked_movement::loaded(&initial, program);
    let result = resolved(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Fused(event) if event.materials.is_empty())),
        "the source's Fusion movement remains distinct from matching a material trait"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    assert!(
        state["objects"]["source"]["counters"]["fusion_counter"].is_null(),
        "CR12.18.3 skips the counter instruction"
    );
}

#[test]
fn hand_only_fusion_into_a_full_ex_rolls_back_the_entire_payment() {
    let (mut game, ability) = position(false, true);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Fusion materials")
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "no hand discard or source movement on failed payment"
    );
}

#[test]
fn fusion_rejects_zero_unsupported_ranges_and_non_hand_activation() {
    for modifier in [
        json!({"n":0_u8}),
        json!({"upto":1_u8}),
        json!({"n":1_u8,"zone":"field"}),
    ] {
        let mut head = json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"trait":"財宝"},"n":1_u8},
            "do":[{"life":{"who":"self","delta":1_i8}}]
        });
        if let Some(zone) = modifier.get("zone") {
            head["zone"] = zone.clone();
        }
        if let Some(count) = modifier.get("upto") {
            head["fusion"].as_object_mut().unwrap().remove("n");
            head["fusion"]["upto"] = count.clone();
        } else {
            head["fusion"]["n"] = modifier["n"].clone();
        }
        let tree = evolution_linked_movement::tree(&[("activate", head)]);
        let error = lowerer().lower(&tree).unwrap_err();
        assert_eq!(error.code, "unsupported_construct");
        assert!(
            error.path.contains("/abilities/0"),
            "reject at the unsupported ability"
        );
    }
}

#[test]
fn event_alternatives_reject_unrelated_or_mismatched_subscriptions() {
    for other in [
        json!({"phase":{"which":"main","whose":"self"}}),
        json!({"played":{"who":"self","sel":{"trait":"食材"}}}),
    ] {
        let tree = evolution_linked_movement::tree(&[(
            "auto",
            json!({
                "on":{"any_of":[{"fused":{"who":"self","material":{"trait":"財宝"}}},other]},
                "do":[{"life":{"who":"self","delta":1_i8}}]
            }),
        )]);
        assert_eq!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct",
            "only the required shared Fusion/Play subscription is admitted"
        );
    }
}

fn pending_count(game: &mut Game) -> usize {
    let mut count = 0_usize;
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return count;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("one per-Fusion occurrence")
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        count = count.checked_add(1).unwrap();
    }
}

fn trait_catalog(source_treasure: bool, material_treasure: bool) -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|raw| {
            let mut row: Value = serde_json::from_str(raw).unwrap();
            let treasure = (row["number"] == "unit-spell" && material_treasure)
                || (row["number"] == "unit-follower" && source_treasure);
            row["faces"][0]["traits"] = if treasure {
                json!(["財宝"])
            } else {
                json!([])
            };
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("fixture.yaml".into(), document)])
            .unwrap(),
    )
}

#[test]
fn fused_filters_use_actual_materials_and_share_their_quota_with_play() {
    for (source_treasure, material_treasure, seat) in
        [(false, true, 1_u32), (true, false, 1), (false, true, 2)]
    {
        let catalog = trait_catalog(source_treasure, material_treasure);
        let source_tree = equipment_played::tree(
            &json!({"activate":{
                "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"upto":2_u8},
                "do":[{"counter":{"on":"this","name":"fusion_counter","delta":1_i8}}]
            }}),
            "unit-follower",
        );
        let observer_tree = equipment_played::tree(
            &json!({"auto":{
                "per_turn":1_u8,
                "on":{"any_of":[
                    {"fused":{"who":"self","material":{"trait":"財宝"}}},
                    {"played":{"who":"self","sel":{"trait":"財宝"}}}
                ]},
                "do":[{"life":{"who":"self","delta":1_i8}}]
            }}),
            "unit-amulet",
        );
        let source = lowerer()
            .lower(&source_tree)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let observer = lowerer()
            .lower(&observer_tree)
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"observer","card":"unit-amulet"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"][actor]["zones"]["hand"] = json!([
            {"id":"source","card":"unit-follower"}, {"id":"h","card":"unit-spell"},
            {"id":"later","card":"unit-spell"}
        ]);
        initial["players"][actor]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "fusion-filter-quota",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &source).unwrap();
        game.load_internal_card("unit-amulet", &observer).unwrap();
        game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "source",
                source.card().abilities[0].node.id,
                PlayerId { seat },
            )
            .unwrap()
        else {
            panic!("Fusion materials")
        };
        // Hand handles precede EX: one hand material and the EX card are fused together.
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0, 2]),
            )
            .unwrap(),
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Fused(event) if event.materials.len() == 2)),
            "one event records both actual materials"
        );
        let expected = usize::from(material_treasure && seat == 1);
        assert_eq!(
            pending_count(&mut game),
            expected,
            "match material trait and actor, independently of the source's trait"
        );
        let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"].clone();
        resolved(game.play_internal_card("later", PlayerId { seat }).unwrap());
        assert_eq!(
            pending_count(&mut game),
            0,
            "the same ability's one-per-turn quota is already consumed by Fusion"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            before
        );
    }
}
#[test]
fn fusion_accepts_a_foreign_owned_card_in_its_controllers_ex_and_returns_it_to_its_owner() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"n":1_u8},
            "do":[{"counter":{"on":"this","name":"fusion_counter","delta":1_i8}}]
        }),
    )]);
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"foreign","card":"unit-spell","owner":"P2"}]);
    let mut game = evolution_linked_movement::loaded(&initial, program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("Fusion material");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("Fusion materials domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the controller's EX supplies the candidate"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Fused(event) if event.controller == (PlayerId { seat: 1 }) && event.materials.len() == 1)));
    assert!(
        result
            .progress
            .facts
            .iter()
            .all(|fact| !matches!(fact, Fact::Discard(_))),
        "an EX payment is not a hand discard"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    assert_eq!(
        state["objects"]["source"]["counters"]["fusion_counter"],
        1_u8
    );
}
#[test]
fn fusion_material_observes_only_its_own_successful_payment_by_the_matching_source() {
    for (has_trait, from, expected) in
        [(true, "hand", 1_usize), (true, "ex", 1), (false, "hand", 0)]
    {
        let source = lowerer()
            .lower(&equipment_played::tree(
                &json!({"activate":{
                    "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"n":1_u8},
                    "do":[{"counter":{"on":"this","name":"fusion_counter","delta":1_i8}}]
                }}),
                "unit-follower",
            ))
            .unwrap();
        let material = lowerer()
            .lower(&equipment_played::tree(
                &json!({"auto":{
                    "on":{"fused_as_material":{"ref":"this","by":{"ability_of":{
                        "who":"self","kind":"follower","trait":"財宝"
                    }}}},
                    "do":[{"life":{"who":"self","delta":1_i8}}]
                }}),
                "unit-spell",
            ))
            .unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"unit-follower"}]);
        if from == "ex" {
            initial["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"material","card":"unit-spell"}]);
        } else {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"material","card":"unit-spell"}),
                    json!({"id":"unpaid","card":"unit-spell"}),
                ]);
        }
        let mut game = Game::new(
            trait_catalog(has_trait, !has_trait),
            &initial,
            &Value::Null,
            &Value::Null,
            "fusion-material-occurrence",
        )
        .unwrap();
        let source = &source["unit-follower"];
        game.load_internal_card("unit-follower", source).unwrap();
        game.load_internal_card("unit-spell", &material["unit-spell"])
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "source",
                source.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("Fusion material decision");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("Fusion cost materials");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            pending_count(&mut game),
            expected,
            "only the selected material can see a Fusion by the matching follower"
        );
    }
}
