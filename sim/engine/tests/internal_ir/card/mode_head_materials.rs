#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected question invalidates the synthetic material scenario."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn fanfare(head: Value) -> Result<(), lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = head;
    lowerer().lower(&tree).map(|_| ())
}

fn banish_two() -> Value {
    json!({"banish":{"sel":{"who":"self","zone":"cemetery","orig_cost_eq":1},"n":2}})
}

#[test]
fn a_triggered_head_material_cost_sits_beside_fee_free_modes() {
    fanfare(
        json!({"cost":banish_two(),"do":[{"choice":{"upto":2,"options":[
            [{"damage":{"to":{"leaders":"opponents"},"n":2}}],
            [{"draw":{"who":"self","n":1}}]
        ]}}]}),
    )
    .unwrap();
}

#[test]
fn a_mode_material_fee_that_may_overlap_the_head_cost_is_still_rejected() {
    let mode_material =
        json!({"banish":{"sel":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}});
    fanfare(
        json!({"cost":banish_two(),"do":[{"choice":{"upto":2,"options":[
            [{"damage":{"to":{"leaders":"opponents"},"n":2}}],
            [{"pay":{"cost":mode_material,"do":[{"draw":{"who":"self","n":1}}]}}]
        ]}}]}),
    )
    .unwrap_err();
}

/// `unit-follower` is a ナイトメア follower and `other-follower` a ビショップ one.
fn classed_catalog() -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let follower = rows
        .iter_mut()
        .find(|row| row["number"] == "unit-follower")
        .unwrap();
    follower["faces"][0]["card_class"] = json!("ナイトメア");
    let mut other = follower.clone();
    other["number"] = json!("other-follower");
    other["faces"][0]["name"] = json!("other-follower");
    other["faces"][0]["card_class"] = json!("ビショップ");
    rows.push(other);
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let extra = json!({"format":1,"kind":"effect_set","cards":{"other-follower":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[("unit.yaml".into(), document), ("other.yaml".into(), extra)],
        )
        .unwrap(),
    )
}

/// Activates a field ability whose head pays `head` and whose every mode pays one of `fees`.
fn activate_with_fees(head: &Value, fees: &[Value], zones: &Value) -> Value {
    let paid =
        |cost: &Value| json!([{"pay":{"cost":cost,"do":[{"life":{"who":"self","delta":1}}]}}]);
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":head,"do":[{"choice":{"n":fees.len(),
        "options":fees.iter().map(paid).collect::<Vec<_>>()}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for (zone, objects) in zones.as_object().unwrap() {
        initial["players"]["P1"]["zones"][zone] = objects.clone();
    }
    let mut game = Game::new(
        classed_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "disjoint-materials",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-spell", "unit-amulet", "other-follower"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let modes = (1..=fees.len())
        .map(|mode| ModeId(u64::try_from(mode).unwrap()))
        .collect::<Vec<_>>();
    let mut step = Some(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Modes { .. } => Answer::Modes(modes.clone()),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(d) => Answer::CostMaterials(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                _ => panic!("unexpected material question: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "no trigger remains after the activation"
        );
        return game.projection(View::Referee).unwrap();
    }
    panic!("unbounded material scenario");
}

fn banished(state: &Value) -> Vec<String> {
    let mut ids: Vec<String> = serde_json::from_value(state["P1"]["banish"].clone()).unwrap();
    ids.sort();
    ids
}

#[test]
fn a_head_material_in_another_zone_joins_a_mode_material_fee() {
    let head = json!({"banish":{"sel":{"who":"self","zone":"ex","kind":"amulet"},"n":1}});
    let fee = json!({"banish":{"sel":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}});
    let state = activate_with_fees(
        &head,
        &[fee],
        &json!({"ex":[{"id":"head","card":"unit-amulet"}],
                "cemetery":[{"id":"fee","card":"other-follower"}]}),
    );
    assert_eq!(banished(&state), ["fee", "head"]);
    assert_eq!(state["P1"]["leader"]["life"], 21);
}

#[test]
fn two_mode_fees_of_exclusive_classes_are_both_paid() {
    let class = |negate: bool| {
        let key = if negate { "not_class" } else { "class" };
        json!({"banish":{"sel":{"who":"self","zone":"cemetery","kind":"follower",key:"ナイトメア"},"n":1}})
    };
    let state = activate_with_fees(
        &json!({"pp":0}),
        &[class(false), class(true)],
        &json!({"cemetery":[{"id":"night","card":"unit-follower"},{"id":"other","card":"other-follower"}]}),
    );
    assert_eq!(banished(&state), ["night", "other"]);
    assert_eq!(state["P1"]["leader"]["life"], 22);
}

#[test]
fn a_hand_discard_head_cost_joins_an_ex_mode_material_fee() {
    let head = json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1}});
    let fee = json!({"banish":{"sel":{"who":"self","zone":"ex"},"n":1}});
    let state = activate_with_fees(
        &head,
        &[fee],
        &json!({"hand":[{"id":"discarded","card":"unit-spell"}],
                "ex":[{"id":"stored","card":"unit-amulet"}]}),
    );
    assert_eq!(state["P1"]["cemetery"], json!(["discarded"]));
    assert_eq!(banished(&state), ["stored"]);
    assert_eq!(state["P1"]["leader"]["life"], 21);
}

#[test]
fn inclusive_card_kinds_in_one_zone_may_overlap_and_stay_rejected() {
    // An evolved follower is also a follower, so these two pools can meet.
    let kind = |kind: &str| json!({"banish":{"sel":{"who":"self","zone":"ex","kind":kind},"n":1}});
    let paid =
        |cost: Value| json!([{"pay":{"cost":cost,"do":[{"life":{"who":"self","delta":1}}]}}]);
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"do":[{"choice":{"n":2,
        "options":[paid(kind("follower")), paid(kind("evolve_follower"))]}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let program = lowerer().lower(&tree).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "kinds",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-follower", &program["unit-follower"])
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("several selectable mode material fees"),
        "the shared-pool gate rejects it: {error:?}"
    );
}

#[test]
fn kinds_with_different_printed_bases_are_both_paid() {
    let kind = |kind: &str| json!({"banish":{"sel":{"who":"self","zone":"ex","kind":kind},"n":1}});
    let state = activate_with_fees(
        &json!({"pp":0}),
        &[kind("follower"), kind("spell")],
        &json!({"ex":[{"id":"unit","card":"other-follower"},{"id":"magic","card":"unit-spell"}]}),
    );
    assert_eq!(banished(&state), ["magic", "unit"]);
    assert_eq!(state["P1"]["leader"]["life"], 22);
}
