use super::{CURRENT_FACE, current_inputs};
use crate::EngineFailure;
use crate::catalog::Catalog;
use crate::game::Game;
use crate::ir::lowering::{ExpandedLowerer, PrintedKeyword};
use crate::ir::{
    AbilityOrigin, Binding, CardProgram, Effect, KeywordId, ObjectOperand, PlayerRef, Polarity,
    WellFormedCardProgram, ZoneKind,
};
use alloc::{collections::BTreeSet, sync::Arc};
use serde_json::{Value, json};
use std::{env::var_os, fs::read_to_string};

#[path = "current_keywords/runtime.rs"]
mod runtime;

const KEYWORD_FACE: &str = "f:0a2de2c8ea0d5629b3f899fc2cc52c88";

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
    let evidence = &checked.declaration_sidecar["/cards/BP01-138/faces/f:0a2de2c8ea0d5629b3f899fc2cc52c88/abilities/0"];
    assert_eq!(
        evidence.keywords(),
        [PrintedKeyword::Ward, PrintedKeyword::Aura]
    );
    assert_eq!(evidence.source.face, "f:0a2de2c8ea0d5629b3f899fc2cc52c88");
    assert_eq!(
        evidence.source.scope,
        "keyword_declaration:/cards/BP01-138/faces/f:0a2de2c8ea0d5629b3f899fc2cc52c88/abilities/0"
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
    let candidate = lowerer.lower_current(&input).unwrap();
    assert_eq!(candidate.program.card().abilities.len(), 2);
    for (index, ability) in candidate.program.card().abilities.iter().enumerate() {
        assert_eq!(
            ability.source.author_path,
            format!(
                "/cards/BP01-138/faces/f:0a2de2c8ea0d5629b3f899fc2cc52c88/abilities/0/keywords/{index}"
            )
        );
        assert_eq!(ability.source.scope, evidence.source.scope);
        assert!(candidate.source_at(&ability.source.author_path).is_none());
        assert_eq!(
            candidate
                .declaration_at(&ability.source.author_path)
                .unwrap()
                .reference,
            evidence.reference
        );
    }
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let selected = Catalog::from_current_selection(
        &snapshot,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &current_inputs()["keywords"],
    )
    .unwrap();
    Game::from_selected_effects(
        selected,
        &json!({}),
        &Value::Null,
        &Value::Null,
        "keyword-admission",
    )
    .unwrap();
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
        .remove("faces")
        .unwrap();
    input["tree"]["cards"]["BP01-138"]["abilities"] = abilities[&face]["abilities"].clone();
    input["declaration_sidecar"][0]["path"] = json!(["cards", "BP01-138", "abilities", 0_u64]);
    let checked = ExpandedLowerer::new()
        .unwrap()
        .check_current(&input)
        .unwrap();
    let path = "/cards/BP01-138/abilities/0";
    let evidence = &checked.declaration_sidecar[path];
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
    omitted["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"] = json!([]);
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
            json!([
                "cards",
                "BP01-138",
                "faces",
                KEYWORD_FACE,
                "abilities",
                1_u64
            ]),
        ),
        (
            "/tree/cards/BP01-138/faces/f:0a2de2c8ea0d5629b3f899fc2cc52c88/abilities/0/line",
            json!(2_u64),
        ),
        (
            "/tree/cards/BP01-138/faces/f:0a2de2c8ea0d5629b3f899fc2cc52c88/abilities/0/keywords",
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
    section["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"][0]["section"] =
        json!(1_u64);
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
            json!(["drain"]),
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
        ("/declaration_sidecar/0/path/5", json!(-1_i64)),
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
    let mut entry =
        input["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"][0].clone();
    entry["sources"][0]["binding_id"] = json!(format!("bind:{}", "e".repeat(64)));
    entry["sources"][0]["occurrence"]["owner"]["revision_id"] = json!(revision);
    let mut evidence = input["declaration_sidecar"][0].clone();
    evidence["path"] = json!(["cards", "BP01-138", "faces", face, "abilities", 0_u64]);
    evidence["source_use"] = entry["sources"][0].clone();
    evidence["face_id"] = json!(face);
    let mut obligation = input["source_inventory"]["non_body"][0].clone();
    obligation["source_use"] = evidence["source_use"].clone();
    obligation["face_id"] = json!(face);
    input["tree"]["cards"]["BP01-138"]["faces"][&face] = json!({"abilities":[entry]});
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
    let candidate = lowerer.lower_current(&input).unwrap();
    assert_eq!(candidate.program.card().abilities.len(), 4);
    assert_eq!(
        candidate
            .program
            .card()
            .abilities
            .iter()
            .map(|ability| ability.node.scope)
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    let mut omitted = input.clone();
    omitted["tree"]["cards"]["BP01-138"]["faces"]
        .as_object_mut()
        .unwrap()
        .remove(&format!("f:{}", "e".repeat(32)));
    omitted["declaration_sidecar"].as_array_mut().unwrap().pop();
    let mut borrowed = input.clone();
    borrowed["declaration_sidecar"][1]["source_use"] =
        input["declaration_sidecar"][0]["source_use"].clone();
    let mut container = input.clone();
    let face = format!("f:{}", "e".repeat(32));
    container["tree"]["cards"]["BP01-138"]["faces"][&face]["abilities"][0] =
        input["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"][0].clone();
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

fn snapshot() -> String {
    read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap()
}

fn admission(selection: &Value) -> crate::Result<Game> {
    let selected = Catalog::from_current_selection(
        &snapshot(),
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        selection,
    )?;
    Game::from_selected_effects(
        selected,
        &json!({}),
        &Value::Null,
        &Value::Null,
        "keyword-admission",
    )
}

#[test]
fn real_advance_printings_lower_both_keywords_and_the_complete_fanfare() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for number in ["BP19-P32", "BP19-114"] {
        let selection = &current_inputs()["keyword_fanfare"][number];
        let input = &selection["authors"][0]["input"];
        let candidate = lowerer.lower_current(input).unwrap();
        let abilities = &candidate.program.card().abilities;
        assert_eq!(abilities.len(), 3);
        assert_eq!(candidate.source_sidecar.len(), 1);
        assert_eq!(candidate.declaration_sidecar.len(), 1);
        for (index, keyword) in [KeywordId::Ward, KeywordId::Aura, KeywordId::Fanfare]
            .into_iter()
            .enumerate()
        {
            assert!(
                matches!(&abilities[index].node.source.origin, AbilityOrigin::Keyword { keyword: found, origin }
                if *found == keyword && matches!(origin.as_ref(), AbilityOrigin::Printed { face, line }
                    if face.0 == abilities[index].source.face && line.0 == u64::from(abilities[index].source.line)))
            );
        }
        assert!(
            abilities[2]
                .node
                .body
                .as_ref()
                .is_some_and(|body| !body.steps.is_empty())
        );
        assert_eq!(
            abilities
                .iter()
                .map(|ability| ability.node.scope)
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
        admission(selection).unwrap();
        let face = input["declaration_sidecar"][0]["face_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut missing = input.clone();
        missing["tree"]["cards"][number]["faces"][&face]["abilities"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        missing["declaration_sidecar"] = json!([]);
        // Keep the remaining body's sidecar path aligned so only keyword closure can fail.
        missing["sidecar"][0]["path"] = json!(["cards", number, "faces", face, "abilities", 0_u64]);
        assert_eq!(
            lowerer.lower_current(&missing).unwrap_err().code,
            "current_keyword_closure_mismatch"
        );
    }
}

#[test]
fn runtime_rejects_changed_permission_headers_expansions_and_source_chains() {
    let input = input();
    let lowerer = ExpandedLowerer::new().unwrap();
    let card = lowerer.lower_current(&input).unwrap();
    let original = raw_keyword_coordinates(card.program.card());
    let baseline = WellFormedCardProgram::check(original.clone()).unwrap();
    let unproven = raw_keyword_game()
        .load_internal_card("BP01-138", &baseline)
        .unwrap_err();
    assert!(
        matches!(unproven, EngineFailure::Unsupported(reason) if reason.contains("requires current declaration proof"))
    );
    let mut candidates = Vec::new();
    let mut zone = original.clone();
    zone.abilities[0].node.active_zones = vec![ZoneKind::Hand];
    candidates.push(zone);
    let mut holder = original.clone();
    holder.abilities[0].node.source.holder = ObjectOperand::Bound {
        binding: Binding::new(1, holder.abilities[0].node.scope),
    };
    candidates.push(holder);
    let mut controller = original.clone();
    controller.abilities[0].node.source.controller = PlayerRef::Owner {};
    candidates.push(controller);
    let mut missing = original.clone();
    missing.abilities[0].node.declarations.clear();
    candidates.push(missing);
    let mut extra = original.clone();
    extra.abilities[0]
        .node
        .declarations
        .push(original.abilities[0].node.declarations[0].clone());
    candidates.push(extra);
    let mut rule = original.clone();
    if let Effect::Permission { rule, .. } = &mut rule.abilities[0].node.declarations[0] {
        rule.0 = 0;
    }
    candidates.push(rule);
    let mut polarity = original.clone();
    if let Effect::Permission { polarity, .. } = &mut polarity.abilities[0].node.declarations[0] {
        *polarity = Polarity::Allow;
    }
    candidates.push(polarity);
    let mut nested = original.clone();
    if let AbilityOrigin::Keyword { origin, .. } = &mut nested.abilities[0].node.source.origin {
        **origin = original.abilities[1].node.source.origin.clone();
    }
    candidates.push(nested);
    let mut unsupported = original;
    if let AbilityOrigin::Keyword { keyword, .. } = &mut unsupported.abilities[0].node.source.origin
    {
        *keyword = KeywordId::Storm;
    }
    candidates.push(unsupported);
    for candidate in candidates {
        if let Ok(checked) = WellFormedCardProgram::check(candidate) {
            let failure = raw_keyword_game()
                .load_internal_card("BP01-138", &checked)
                .unwrap_err();
            assert!(
                matches!(failure, EngineFailure::Unsupported(reason) if reason.contains("whole-card ability"))
            );
        }
    }
}

fn raw_keyword_coordinates(card: &CardProgram) -> CardProgram {
    // This synthetic coordinate probe reaches the raw loader, without granting current proof.
    let mut card = card.clone();
    for ability in &mut card.abilities {
        ability.source.card_id.clone_from(&ability.source.card_no);
        ability.source.face = "base".into();
        ability.source.owner = "card".into();
        ability.source.field = "text".into();
        ability.source.fragment = None;
        if let AbilityOrigin::Keyword { origin, .. } = &mut ability.node.source.origin
            && let AbilityOrigin::Printed { face, .. } = origin.as_mut()
        {
            face.0 = "base".into();
        }
    }
    card
}

fn raw_keyword_game() -> Game {
    raw_keyword_game_for("BP01-138")
}

fn raw_keyword_game_for(number: &str) -> Game {
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{
        number:{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Catalog::from_documents(
        &snapshot(),
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &[("synthetic.yaml".into(), document)],
    )
    .unwrap();
    Game::new(
        Arc::new(catalog),
        &json!({}),
        &Value::Null,
        &Value::Null,
        "rejection",
    )
    .unwrap()
}

#[test]
fn body_and_declaration_obligations_are_consumed_independently() {
    let mut input = input();
    let body = &current_inputs()["accepted"];
    let mut entry =
        body["tree"]["cards"]["BP01-009"]["faces"][CURRENT_FACE]["abilities"][0].clone();
    let mut source = input["declaration_sidecar"][0]["source_use"].clone();
    source["binding_id"] = json!(format!("bind:{}", "e".repeat(64)));
    source["ability_scope"] = json!("ability_0");
    entry["sources"] = json!([source]);
    let mut evidence = body["sidecar"][0].clone();
    evidence["path"] = json!([
        "cards",
        "BP01-138",
        "faces",
        KEYWORD_FACE,
        "abilities",
        1_u64
    ]);
    evidence["source_use"] = source.clone();
    evidence["input_hash"] = input["declaration_sidecar"][0]["input_hash"].clone();
    evidence["source_input_hash"] = input["declaration_sidecar"][0]["source_input_hash"].clone();
    let mut obligation = body["source_inventory"]["abilities"][0].clone();
    obligation["source_use"] = source;
    input["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"]
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
    missing["tree"]["cards"]["BP01-138"]["faces"][KEYWORD_FACE]["abilities"]
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
