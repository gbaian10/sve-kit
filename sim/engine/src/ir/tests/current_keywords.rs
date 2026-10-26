use super::{current_inputs, selected_catalog};
use crate::ir::lowering::{ExpandedLowerer, PrintedKeyword};
use alloc::collections::BTreeSet;
use serde_json::{Value, json};

fn input() -> Value {
    current_inputs()["keywords"]["authors"][0]["input"].clone()
}

#[test]
fn real_current_keyword_selection_checks_complete_declarations_before_lowering() {
    let input = input();
    let lowerer = ExpandedLowerer::new().unwrap();
    let checked = lowerer.check_current(&input).unwrap();
    assert!(checked.source_sidecar.is_empty());
    assert_eq!(checked.declaration_sidecar.len(), 1);
    let evidence = &checked.declaration_sidecar["/cards/BP01-138/abilities/0"];
    assert_eq!(
        evidence.keywords(),
        [PrintedKeyword::Ward, PrintedKeyword::Aura]
    );
    assert_eq!(evidence.source.face, "f:0a2de2c8ea0d5629b3f899fc2cc52c88");
    assert_eq!(
        evidence.source.scope,
        "keyword_declaration:/cards/BP01-138/abilities/0"
    );
    let reference = serde_json::to_value(&evidence.reference).unwrap();
    assert_eq!(
        reference["source_use"],
        input["declaration_sidecar"][0]["source_use"]
    );
    assert!(reference["source_use"].get("ability_scope").is_none());
    assert!(reference.get("body_version").is_none());
    assert_eq!(
        evidence.input_hash,
        input["declaration_sidecar"][0]["input_hash"]
    );
    assert_ne!(evidence.adapter_recipe, reference["source_adapter_recipe"]);
    // Source reconciliation is a separate success boundary from executable lowering.
    assert_eq!(
        lowerer.lower_current(&input).unwrap_err().code,
        "unsupported_construct"
    );
    selected_catalog(&current_inputs()["keywords"]).unwrap_err();
}

#[test]
fn declaration_containers_and_each_list_element_keep_the_checked_source() {
    let mut input = input();
    let face = input["declaration_sidecar"][0]["face_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let abilities = input["tree"]["cards"]["BP01-138"]
        .as_object_mut()
        .unwrap()
        .remove("abilities")
        .unwrap();
    input["tree"]["cards"]["BP01-138"]["faces"] = json!({&face:{"abilities":abilities}});
    input["declaration_sidecar"][0]["path"] =
        json!(["cards", "BP01-138", "faces", face, "abilities", 0_u64]);
    let checked = ExpandedLowerer::new()
        .unwrap()
        .check_current(&input)
        .unwrap();
    let path = format!("/cards/BP01-138/faces/{face}/abilities/0");
    let evidence = &checked.declaration_sidecar[&path];
    assert_eq!(evidence.source.face, face);
    assert_eq!(evidence.source.scope, format!("keyword_declaration:{path}"));
    assert_eq!(evidence.keywords().len(), 2);
    for index in 0_u8..2 {
        let child = checked
            .declaration_at(&format!("{path}/keywords/{index}"))
            .unwrap();
        assert_eq!(child.source, evidence.source);
    }
    assert!(
        checked
            .declaration_at(&format!("{path}1/keywords/0"))
            .is_none()
    );
}

fn assert_rejected(candidates: Vec<Value>) {
    let lowerer = ExpandedLowerer::new().unwrap();
    for candidate in candidates {
        lowerer.check_current(&candidate).unwrap_err();
        lowerer.lower_current(&candidate).unwrap_err();
    }
}

#[test]
fn declaration_closure_rejects_joint_omission_duplicates_extras_and_inert_metadata() {
    let input = input();
    let mut omitted = input.clone();
    omitted["tree"]["cards"]["BP01-138"]["abilities"] = json!([]);
    omitted["declaration_sidecar"] = json!([]);
    let mut missing = input.clone();
    missing["declaration_sidecar"] = json!([]);
    let mut duplicate = input.clone();
    duplicate["declaration_sidecar"]
        .as_array_mut()
        .unwrap()
        .push(input["declaration_sidecar"][0].clone());
    let mut inventory_duplicate = input.clone();
    inventory_duplicate["source_inventory"]["non_body"]
        .as_array_mut()
        .unwrap()
        .push(input["source_inventory"]["non_body"][0].clone());
    let mut extra = input.clone();
    extra["source_inventory"]["non_body"] = json!([]);
    let mut inert = input.clone();
    inert["source_inventory"]["non_body"] = json!([{
        "kind":"metadata", "binding_id":input["declaration_sidecar"][0]["source_use"]["binding_id"],
        "occurrence": input["declaration_sidecar"][0]["source_use"]["occurrence"], "semantic_key":"pure_reminder.v1"
    }]);
    assert_rejected(vec![
        omitted,
        missing,
        duplicate,
        inventory_duplicate,
        extra,
        inert,
    ]);
}

#[test]
fn complete_declaration_payload_and_all_independent_source_coordinates_must_match() {
    let input = input();
    let mut candidates = Vec::new();
    for (path, value) in [
        (
            "/source_inventory/non_body/0/declaration/keywords",
            json!(["aura", "ward"]),
        ),
        (
            "/source_inventory/non_body/0/frame_id",
            json!(format!("frame:{}", "0".repeat(64))),
        ),
        (
            "/source_inventory/non_body/0/interface_key",
            json!("0".repeat(64)),
        ),
        (
            "/declaration_sidecar/0/declaration/keywords",
            json!(["ward"]),
        ),
        (
            "/declaration_sidecar/0/face_id",
            json!(format!("f:{}", "0".repeat(32))),
        ),
        (
            "/declaration_sidecar/0/source_input_hash",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("/declaration_sidecar/0/input_hash", json!("bad")),
        (
            "/declaration_sidecar/0/expansion_recipe",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        (
            "/declaration_sidecar/0/adapter_recipe",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        (
            "/declaration_sidecar/0/source_adapter_recipe",
            json!(format!("sha256:{}", "0".repeat(64))),
        ),
        (
            "/declaration_sidecar/0/source_use/occurrence/owner/revision_id",
            json!("foreign"),
        ),
        (
            "/declaration_sidecar/0/source_use/occurrence/segments",
            json!([{"start":0_u64,"end":1_u64}]),
        ),
        (
            "/declaration_sidecar/0/source_use/occurrence/line_ordinal",
            json!(1_u64),
        ),
        (
            "/declaration_sidecar/0/path",
            json!(["cards", "BP01-138", "abilities", 1_u64]),
        ),
        ("/tree/cards/BP01-138/abilities/0/line", json!(2_u64)),
        (
            "/tree/cards/BP01-138/abilities/0/keywords",
            json!(["aura", "ward"]),
        ),
        (
            "/source_payload/faces/0/texts/1/source_hash",
            json!("0".repeat(64)),
        ),
    ] {
        let mut candidate = input.clone();
        *candidate.pointer_mut(path).unwrap() = value;
        candidates.push(candidate);
    }
    let mut section = input.clone();
    section["tree"]["cards"]["BP01-138"]["abilities"][0]["section"] = json!(1_u64);
    candidates.push(section);
    let mut ordinal = input;
    ordinal["declaration_sidecar"][0]["source_use"]["occurrence"]["ordinal"] = json!(1_u64);
    candidates.push(ordinal);
    assert_rejected(candidates);
}

#[test]
fn current_internal_keyword_wire_is_closed_and_required() {
    let input = input();
    let mut candidates = Vec::new();
    for (path, value) in [
        ("/source_inventory/non_body/0/kind", json!("unknown")),
        (
            "/source_inventory/non_body/0/declaration/keywords",
            json!(["storm"]),
        ),
        (
            "/source_inventory/non_body/0/declaration/keywords",
            json!(["ward", "ward"]),
        ),
        (
            "/source_inventory/non_body/0/declaration/ports/imports",
            json!({"fake":{}}),
        ),
        ("/declaration_sidecar/0/source_proof", json!("none")),
        ("/declaration_sidecar/0/input_hash", Value::Null),
        ("/declaration_sidecar/0/path/3", json!(-1_i64)),
        (
            "/source_inventory/non_body/0/source_use/occurrence/segments",
            json!([]),
        ),
        (
            "/source_inventory/non_body/0/source_use/occurrence/owner/kind",
            json!("printing_face"),
        ),
    ] {
        let mut candidate = input.clone();
        *candidate.pointer_mut(path).unwrap() = value;
        candidates.push(candidate);
    }
    for field in [
        "kind",
        "source_use",
        "face_id",
        "source_input_hash",
        "input_hash",
        "expansion_recipe",
        "adapter_recipe",
    ] {
        let mut candidate = input.clone();
        candidate["declaration_sidecar"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        candidates.push(candidate);
    }
    for field in ["ability_scope", "body_version"] {
        let mut candidate = input.clone();
        candidate["declaration_sidecar"][0][field] = json!("fake");
        candidates.push(candidate);
    }
    let mut scoped = input.clone();
    scoped["declaration_sidecar"][0]["source_use"]["ability_scope"] = json!("ability_0");
    candidates.push(scoped);
    let mut absent = input;
    absent
        .as_object_mut()
        .unwrap()
        .remove("declaration_sidecar");
    candidates.push(absent);
    assert_rejected(candidates);
}

#[test]
fn metadata_is_inert_and_must_use_the_closed_metadata_variant() {
    let mut body = current_inputs()["accepted"].clone();
    let source = body["sidecar"][0]["source_use"].clone();
    let metadata = json!({"kind":"metadata", "binding_id":source["binding_id"], "occurrence":source["occurrence"], "semantic_key":"metadata.layout.v1"});
    body["source_inventory"]["non_body"] = json!([metadata]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let checked = lowerer.check_current(&body).unwrap();
    assert!(checked.declaration_sidecar.is_empty());
    lowerer.lower_current(&body).unwrap();
    let mut executable = body.clone();
    executable["source_inventory"]["non_body"][0]["declaration"] =
        json!({"keywords":["ward"],"ports":{"imports":{},"exports":{}}});
    let mut unknown = body.clone();
    unknown["source_inventory"]["non_body"][0]["semantic_key"] = json!("unknown.v1");
    let mut no_kind = body;
    no_kind["source_inventory"]["non_body"][0]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    assert_rejected(vec![executable, unknown, no_kind]);
}

fn synthetic_two_faces() -> Value {
    // This is a D0 closure probe; only the unmodified BP01-138 fixture proves C0 admission.
    let mut input = input();
    let face = format!("f:{}", "e".repeat(32));
    let revision = "revision:synthetic-keyword-back";
    let mut back = input["source_payload"]["faces"][0].clone();
    back["face_id"] = json!(face);
    back["revision_id"] = json!(revision);
    input["source_payload"]["faces"]
        .as_array_mut()
        .unwrap()
        .push(back);
    let mut entry = input["tree"]["cards"]["BP01-138"]["abilities"][0].clone();
    entry["sources"][0]["binding_id"] = json!(format!("bind:{}", "e".repeat(64)));
    entry["sources"][0]["occurrence"]["owner"]["revision_id"] = json!(revision);
    let mut evidence = input["declaration_sidecar"][0].clone();
    evidence["path"] = json!(["cards", "BP01-138", "faces", face, "abilities", 0_u64]);
    evidence["source_use"] = entry["sources"][0].clone();
    evidence["face_id"] = json!(face);
    let mut obligation = input["source_inventory"]["non_body"][0].clone();
    obligation["source_use"] = evidence["source_use"].clone();
    obligation["face_id"] = json!(face);
    input["tree"]["cards"]["BP01-138"]["faces"] = json!({face:{"abilities":[entry]}});
    input["declaration_sidecar"]
        .as_array_mut()
        .unwrap()
        .push(evidence);
    input["source_inventory"]["non_body"]
        .as_array_mut()
        .unwrap()
        .push(obligation);
    input
}

#[test]
fn same_keywords_on_two_faces_consume_separate_obligations() {
    let input = synthetic_two_faces();
    let lowerer = ExpandedLowerer::new().unwrap();
    let checked = lowerer.check_current(&input).unwrap();
    let faces: BTreeSet<_> = checked
        .declaration_sidecar
        .values()
        .map(|evidence| &evidence.source.face)
        .collect();
    assert_eq!(faces.len(), 2);
    let mut omitted = input.clone();
    omitted["tree"]["cards"]["BP01-138"]["faces"] = json!({});
    omitted["declaration_sidecar"].as_array_mut().unwrap().pop();
    let mut borrowed = input.clone();
    borrowed["declaration_sidecar"][1]["source_use"] =
        input["declaration_sidecar"][0]["source_use"].clone();
    let mut container = input.clone();
    let face = format!("f:{}", "e".repeat(32));
    container["tree"]["cards"]["BP01-138"]["faces"][&face]["abilities"][0] =
        input["tree"]["cards"]["BP01-138"]["abilities"][0].clone();
    container["declaration_sidecar"][1] = input["declaration_sidecar"][0].clone();
    container["declaration_sidecar"][1]["path"] =
        json!(["cards", "BP01-138", "faces", face, "abilities", 0_u64]);
    let mut wrong_face = input.clone();
    wrong_face["declaration_sidecar"][1]["source_use"]["occurrence"]["owner"] =
        input["declaration_sidecar"][0]["source_use"]["occurrence"]["owner"].clone();
    wrong_face["source_inventory"]["non_body"][1]["source_use"] =
        wrong_face["declaration_sidecar"][1]["source_use"].clone();
    wrong_face["tree"]["cards"]["BP01-138"]["faces"][&face]["abilities"][0]["sources"][0] =
        wrong_face["declaration_sidecar"][1]["source_use"].clone();
    assert_rejected(vec![omitted, borrowed, container, wrong_face]);
}

#[test]
fn body_and_declaration_obligations_are_consumed_independently() {
    let mut input = input();
    let body = &current_inputs()["accepted"];
    let mut entry = body["tree"]["cards"]["BP01-009"]["abilities"][0].clone();
    let mut source = input["declaration_sidecar"][0]["source_use"].clone();
    source["binding_id"] = json!(format!("bind:{}", "e".repeat(64)));
    source["ability_scope"] = json!("ability_0");
    entry["sources"] = json!([source]);
    let mut evidence = body["sidecar"][0].clone();
    evidence["path"] = json!(["cards", "BP01-138", "abilities", 1_u64]);
    evidence["source_use"] = source.clone();
    evidence["input_hash"] = input["declaration_sidecar"][0]["input_hash"].clone();
    evidence["source_input_hash"] = input["declaration_sidecar"][0]["source_input_hash"].clone();
    let mut obligation = body["source_inventory"]["abilities"][0].clone();
    obligation["source_use"] = source;
    input["tree"]["cards"]["BP01-138"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    input["sidecar"] = json!([evidence]);
    input["source_inventory"]["abilities"] = json!([obligation]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let checked = lowerer.check_current(&input).unwrap();
    assert_eq!(checked.source_sidecar.len(), 1);
    assert_eq!(checked.declaration_sidecar.len(), 1);
    let mut missing = input.clone();
    missing["tree"]["cards"]["BP01-138"]["abilities"]
        .as_array_mut()
        .unwrap()
        .pop();
    missing["sidecar"] = json!([]);
    let mut bad_hash = input.clone();
    bad_hash["declaration_sidecar"][0]["input_hash"] = json!(format!("sha256:{}", "0".repeat(64)));
    let mut wrong_recipe = input;
    for path in [
        "/declaration_sidecar/0/source_adapter_recipe",
        "/source_inventory/non_body/0/source_adapter_recipe",
    ] {
        *wrong_recipe.pointer_mut(path).unwrap() = json!(format!("sha256:{}", "0".repeat(64)));
    }
    assert_rejected(vec![missing, bad_hash, wrong_recipe]);
}
