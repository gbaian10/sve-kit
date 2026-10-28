use super::*;
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer, NonBodySource, validator};
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

#[path = "../../../tests/support/expanded.rs"]
mod expanded;

#[path = "current_keywords.rs"]
mod current_keywords;

use crate::catalog::{Catalog, SelectedEffects};
use std::{
    env::temp_dir,
    fs::{create_dir_all, remove_dir_all},
    path::PathBuf,
    process::{Command, id},
    sync::OnceLock,
};

const CURRENT_FACE: &str = "f:8708790ca2995a0f9f9e5ffd1af52668";

fn current_inputs() -> &'static Value {
    static CURRENT: OnceLock<Value> = OnceLock::new();
    CURRENT.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("uv")
            .args(["run", "--directory"])
            .arg(root.join("carddb"))
            .args([
                "--isolated",
                "--locked",
                "--no-default-groups",
                "python",
                "-c",
                "
import json
from tempfile import TemporaryDirectory
from pathlib import Path
from tests.support.current_lowering_fixtures import current_lowering_inputs
with TemporaryDirectory() as temporary:
    print(json.dumps(current_lowering_inputs(Path(temporary))))
",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "current C0 fixture failed: {}",
            output.status
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn selected_catalog(input: &Value) -> crate::Result<SelectedEffects> {
    Catalog::from_current_selection(
        r#"{"number":"BP01-009","faces":[{"card_type":"フォロワー","name":"synthetic switch"}]}"#,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        input,
    )
}

#[test]
fn batch_switch_selects_current_card_and_removes_its_prototype() {
    let switch = &current_inputs()["switch"];
    let before = selected_catalog(&switch["before"]).unwrap();
    assert_eq!(before.prototypes.authored_count(), 1);
    assert!(before.current.is_empty());
    let after = selected_catalog(&switch["after"]).unwrap();
    assert_eq!(after.prototypes.authored_count(), 0);
    after.prototypes.program("BP01-009").unwrap_err();
    let current = &after.current["BP01-009"];
    assert_eq!(current.program.card().abilities.len(), 1);
    assert_eq!(current.card_id, "c:9c5459446e0254818099800b4208cd96");
}

#[test]
fn batch_switch_rejects_missing_abilities_and_overlapping_versions() {
    let switch = &current_inputs()["switch"];
    assert_eq!(switch["overlap"], "prototype_author_overlap");
    let missing = selected_catalog(&switch["missing"]).unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("source_ability_closure_mismatch")
    );
    let mut overlap = switch["after"].clone();
    overlap["documents"] = switch["before"]["documents"].clone();
    selected_catalog(&overlap).unwrap_err();
    let mut missing_body = switch["after"].clone();
    missing_body["authors"][0]["input"]["tree"]["cards"]["BP01-009"]["faces"][CURRENT_FACE]["abilities"] =
        json!([]);
    missing_body["authors"][0]["input"]["sidecar"] = json!([]);
    selected_catalog(&missing_body).unwrap_err();
    assert_legacy_loader_refuses_current_inventory();
}

fn assert_legacy_loader_refuses_current_inventory() {
    let root = temp_dir().join(format!("sve-current-selection-{}", id()));
    create_dir_all(root.join("rules")).unwrap();
    create_dir_all(root.join("rules/effects-index")).unwrap();
    let rejected = Catalog::load(&root.join("absent.jsonl"), &root).unwrap_err();
    assert!(rejected.to_string().contains("from_current_selection"));
    remove_dir_all(root).unwrap();
}

fn assert_current_sources(admitted: &CurrentCardProgram, input: &Value, face: &str) {
    let evidence = admitted
        .source_at(
            "/cards/BP01-009/faces/f:8708790ca2995a0f9f9e5ffd1af52668/abilities/0/evolve/cost/pp",
        )
        .unwrap();
    assert_eq!(evidence.source.card_id, admitted.card_id);
    assert_eq!(evidence.source.face, face);
    assert_eq!(evidence.source.field, "effect");
    assert_eq!(evidence.source.section, None);
    assert_eq!(evidence.source.line, 1);
    assert_eq!(
        evidence.source.fragment.as_deref(),
        evidence.source_use["binding_id"].as_str()
    );
    assert_eq!(
        evidence.source.owner,
        evidence.source_use["occurrence"]["owner"].to_string()
    );
    assert_eq!(evidence.input_hash, input["sidecar"][0]["input_hash"]);
    assert_eq!(
        evidence.source_input_hash,
        input["sidecar"][0]["source_input_hash"]
    );
    assert!(
        admitted
            .source_at("/cards/BP01-009/faces/f:8708790ca2995a0f9f9e5ffd1af52668/abilities/01")
            .is_none()
    );
    let requirements = Requirements::derive(&Located {
        source: admitted.program.card().abilities[0].source.clone(),
        node: admitted.program.card().clone(),
    });
    for requirement in requirements.required {
        for location in requirement.locations {
            let checked = admitted.source_at(&location.source.author_path).unwrap();
            assert_eq!(checked.source, location.source);
        }
    }
}

#[test]
fn current_reader_hands_permanent_owner_and_token_selection_to_lowering() {
    let input = &current_inputs()["accepted"];
    let lowerer = ExpandedLowerer::new().unwrap();
    let admitted = lowerer.lower_current(input).unwrap();
    let face = "f:8708790ca2995a0f9f9e5ffd1af52668";
    let revision = "rev:v1:c9b82c4e4e8df5c8ab2b4975c3058c7e0ffdd179e76429d4e47f4c557cac7304";
    assert_eq!(admitted.card_id, "c:9c5459446e0254818099800b4208cd96");
    assert_eq!(admitted.card_no, "BP01-009");
    assert_eq!(admitted.source_version, input["source_version"]);
    assert_eq!(admitted.source_payload["token_links"], json!([]));
    assert_eq!(admitted.expanded, input["tree"]);
    assert_current_sources(&admitted, input, face);
    let ability = &admitted.program.card().abilities[0];
    assert_eq!(ability.source.card_id, admitted.card_id);
    assert_eq!(ability.source.card_no, admitted.card_no);
    assert_eq!(ability.source.face, face);
    assert_eq!(
        serde_json::from_str::<Value>(&ability.source.owner).unwrap(),
        json!({"kind":"face_revision", "revision_id":revision})
    );
    assert_eq!(ability.source.field, "effect");
    let body = ability.node.body.as_ref().unwrap();
    assert_eq!(body.steps[0].source.card_id, admitted.card_id);
    assert_eq!(body.steps[0].source.face, face);
    let AbilityOrigin::Keyword { origin, .. } = &ability.node.source.origin else {
        panic!("evolution");
    };
    assert!(
        matches!(origin.as_ref(), AbilityOrigin::Printed { face: owner, .. } if owner.0 == face)
    );
    let mut container = input.clone();
    let abilities = container["tree"]["cards"]["BP01-009"]
        .as_object_mut()
        .unwrap()
        .remove("faces")
        .unwrap();
    container["tree"]["cards"]["BP01-009"]["abilities"] = abilities[face]["abilities"].clone();
    container["sidecar"][0]["path"] = json!(["cards", "BP01-009", "abilities", 0_u64]);
    let nested = lowerer.lower_current(&container).unwrap();
    assert_eq!(nested.program.card().abilities[0].source.face, face);
    assert!(
        nested.program.card().abilities[0]
            .source
            .author_path
            .starts_with("/cards/BP01-009/abilities/")
    );
}

#[test]
fn provisional_current_printing_and_unresolved_handoffs_cannot_lower() {
    let inputs = current_inputs();
    assert_eq!(inputs["rejected"]["reason"], "provisional_record_key");
    let lowerer = ExpandedLowerer::new().unwrap();
    assert_eq!(
        lowerer.lower_current(&inputs["rejected"]).unwrap_err().code,
        "invalid_current_identity"
    );
    for pointer in [
        "/card_id",
        "/source_payload/faces/0/face_id",
        "/source_payload/faces/0/revision_id",
    ] {
        let mut input = inputs["accepted"].clone();
        *input.pointer_mut(pointer).unwrap() = json!("unresolved");
        assert_eq!(
            lowerer.lower_current(&input).unwrap_err().code,
            "invalid_current_identity"
        );
    }
    let mut input = inputs["accepted"].clone();
    input["source_payload"]["token_links"] = json!([{"token_face_id":"unknown"}]);
    assert_eq!(
        lowerer.lower_current(&input).unwrap_err().code,
        "unsupported_token_selection"
    );
    for pointer in [
        "/sidecar/0/source_proof",
        "/sidecar/0/source_input_hash",
        "/sidecar/0/input_hash",
        "/sidecar/0/source_use/occurrence/owner/revision_id",
        "/sidecar/0/path/1",
        "/tree/cards/BP01-009/faces/f:8708790ca2995a0f9f9e5ffd1af52668/abilities/0/sources/0/occurrence/owner/revision_id",
        "/tree/cards/BP01-009/faces/f:8708790ca2995a0f9f9e5ffd1af52668/abilities/0/sources/0/occurrence/line_ordinal",
    ] {
        let mut mismatched = inputs["accepted"].clone();
        *mismatched.pointer_mut(pointer).unwrap() = json!("mismatch");
        assert_eq!(
            lowerer.lower_current(&mismatched).unwrap_err().code,
            "invalid_current_source"
        );
    }
    let mut other_hash = inputs["accepted"].clone();
    other_hash["sidecar"][0]["source_input_hash"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert_eq!(
        lowerer.lower_current(&other_hash).unwrap_err().code,
        "invalid_current_source"
    );
    let mut zero_line = inputs["accepted"].clone();
    zero_line["tree"]["cards"]["BP01-009"]["faces"][CURRENT_FACE]["abilities"][0]["line"] =
        json!(0_u32);
    for use_path in [
        "/tree/cards/BP01-009/faces/f:8708790ca2995a0f9f9e5ffd1af52668/abilities/0/sources/0/occurrence",
        "/sidecar/0/source_use/occurrence",
    ] {
        zero_line
            .pointer_mut(use_path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("line_ordinal");
    }
    assert_eq!(
        lowerer.lower_current(&zero_line).unwrap_err().code,
        "invalid_current_source"
    );
    let mut missing = inputs["accepted"].clone();
    missing["sidecar"] = json!([]);
    assert_eq!(
        lowerer.lower_current(&missing).unwrap_err().code,
        "invalid_current_source"
    );
    let mut duplicate = inputs["accepted"].clone();
    duplicate["sidecar"]
        .as_array_mut()
        .unwrap()
        .push(inputs["accepted"]["sidecar"][0].clone());
    assert_eq!(
        lowerer.lower_current(&duplicate).unwrap_err().code,
        "invalid_current_source"
    );
}

fn synthetic_current_closure() -> Value {
    // Synthetic obligations exercise D0 beyond the entry-only current source adapter.
    let mut input = current_inputs()["accepted"].clone();
    let face = format!("f:{}", "e".repeat(32));
    let revision = "revision:synthetic-back";
    let mut back = input["source_payload"]["faces"][0].clone();
    back["face_id"] = json!(face);
    back["revision_id"] = json!(revision);
    let mut text = back["texts"][1].clone();
    text["field"] = json!("section");
    text["ordinal"] = json!(0_u64);
    back["texts"].as_array_mut().unwrap().push(text);
    input["source_payload"]["faces"]
        .as_array_mut()
        .unwrap()
        .push(back);
    let mut uses = input["sidecar"][0]["source_use"].clone();
    uses["binding_id"] = json!(format!("bind:{}", "e".repeat(64)));
    uses["occurrence"]["owner"]["revision_id"] = json!(revision);
    uses["occurrence"]["field"] = json!("section");
    uses["occurrence"]["ordinal"] = json!(0_u64);
    let mut abilities = Vec::new();
    for index in 0_u64..2 {
        uses["ability_scope"] = json!(format!("ability_{index}"));
        let body = if index == 0 {
            json!({"ports":{"imports":{},"exports":{}}, "spell":{"do":[{"choice":{"n":1_u64,"options":[[{"draw":{"n":1_u64}}],[{"draw":{"n":2_u64}}]]}}]}})
        } else {
            json!({"ports":{"imports":{},"exports":{}}, "fanfare":{"do":[{"draw":{"n":2_u64}}]}})
        };
        let mut ability = body.clone();
        ability["line"] = json!(1_u64);
        ability["section"] = json!(0_u64);
        ability["sources"] = json!([uses]);
        abilities.push(ability);
        let mut evidence = input["sidecar"][0].clone();
        evidence["path"] = json!(["cards", "BP01-009", "faces", face, "abilities", index]);
        evidence["source_use"] = uses.clone();
        input["sidecar"].as_array_mut().unwrap().push(evidence);
        input["source_inventory"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source_use":uses,"body":body}));
    }
    input["tree"]["cards"]["BP01-009"]["faces"][&face] = json!({"abilities":abilities});
    input
}

#[test]
fn complete_current_inventory_covers_faces_sections_siblings_and_all_modes() {
    let input = synthetic_current_closure();
    let admitted = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input)
        .unwrap();
    assert_eq!(admitted.program.card().abilities.len(), 3);
    assert_eq!(admitted.source_sidecar.len(), 3);
    assert_eq!(admitted.non_body_sources, Vec::<NonBodySource>::new());
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &admitted.program.card().abilities[1]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps[0]
        .node
    else {
        panic!("all choice modes must lower");
    };
    assert_eq!(modes.len(), 2);
    let mode_path = format!(
        "/cards/BP01-009/faces/f:{}/abilities/0/spell/do/0/choice/options/1/0",
        "e".repeat(32)
    );
    let evidence = admitted.source_at(&mode_path).unwrap();
    assert_eq!(evidence.source.field, "section");
    assert_eq!(evidence.source.section.as_deref(), Some("0"));
    assert_eq!(evidence.source.face, format!("f:{}", "e".repeat(32)));
}

#[test]
fn current_inventory_rejects_joint_omissions_extras_and_changed_mode_bodies() {
    let input = synthetic_current_closure();
    let lowerer = ExpandedLowerer::new().unwrap();
    let face_path = format!("/tree/cards/BP01-009/faces/f:{}/abilities", "e".repeat(32));
    let mut missing = input.clone();
    missing
        .pointer_mut(&face_path)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .pop();
    missing["sidecar"].as_array_mut().unwrap().pop();
    let mut extra = input.clone();
    extra["source_inventory"]["abilities"]
        .as_array_mut()
        .unwrap()
        .pop();
    let mut duplicate = input.clone();
    duplicate["source_inventory"]["abilities"][2] =
        input["source_inventory"]["abilities"][1].clone();
    let mut mode = input.clone();
    mode.pointer_mut(&face_path).unwrap()[0]["spell"]["do"][0]["choice"]["options"][1] = json!([]);
    let mut occurrence = input.clone();
    occurrence["source_inventory"]["abilities"][0]["source_use"]["occurrence"]["source_hash"] =
        json!("0".repeat(64));
    let mut bad_key = input;
    bad_key["source_inventory"]["abilities"][0]["source_use"]["ability_scope"] = Value::Null;
    for rejected in [missing, extra, duplicate, mode, occurrence, bad_key] {
        assert_eq!(
            lowerer.lower_current(&rejected).unwrap_err().code,
            "current_ability_closure_mismatch"
        );
    }
}

#[test]
fn expanded_dynamic_scope_rejects_a_parameter_that_authored_accepts() {
    let expanded = validator("expanded").unwrap();
    let mut tree = expanded::tree().clone();
    assert!(expanded.is_valid(&tree));
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["define"]["X"]["minus"][1] =
        json!({"param":"offset"});
    assert!(validator("authored").unwrap().is_valid(&tree));
    assert!(!expanded.is_valid(&tree));
}

#[test]
fn keyword_sources_are_scope_free_and_keep_their_own_coordinate() {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut declaration = seed["abilities"][0].clone();
    declaration.as_object_mut().unwrap().remove("fanfare");
    declaration["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("ability_scope");
    declaration["keywords"] = json!(["quick"]);
    seed["abilities"] = json!([declaration.clone()]);
    tree["cards"] = json!({"synthetic": seed});
    let authored = validator("authored").unwrap();
    let expanded = validator("expanded").unwrap();
    assert!(authored.is_valid(&tree));
    assert!(expanded.is_valid(&tree));
    let lowerer = ExpandedLowerer::new().unwrap();
    let programs = lowerer.lower(&tree).unwrap();
    let source = &programs["synthetic"].card().abilities[0].source;
    assert_eq!(
        source.scope,
        "keyword_declaration:/cards/synthetic/abilities/0"
    );
    assert_eq!(source.author_path, "/cards/synthetic/abilities/0");
    assert_eq!(
        source.fragment.as_deref(),
        declaration["sources"][0]["binding_id"].as_str()
    );

    let pointer = "/cards/synthetic/abilities/0";
    let mut old = tree.clone();
    old.pointer_mut(pointer).unwrap()["sources"][0]["ability_scope"] = json!("ability_0");
    let mut ordinary = tree.clone();
    ordinary
        .pointer_mut(pointer)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("keywords");
    ordinary.pointer_mut(pointer).unwrap()["spell"] = json!({"do":[]});
    let mut imports = tree.clone();
    imports.pointer_mut(pointer).unwrap()["imports"] =
        json!({"p":{"binding_id":"b","ability_scope":"ability_0","port":"p"}});
    let mut port = tree.clone();
    port.pointer_mut(pointer).unwrap()["ports"]["exports"] = json!({"p":{"path":[],"bind":"x"}});
    let mut duplicate = tree.clone();
    duplicate.pointer_mut(pointer).unwrap()["keywords"] = json!(["quick", "quick"]);
    let mut generated = tree.clone();
    generated.pointer_mut(pointer).unwrap()["generated_by"] = json!({});
    for rejected in [old, ordinary, imports, port, duplicate, generated] {
        assert!(!authored.is_valid(&rejected));
        assert!(!expanded.is_valid(&rejected));
        lowerer.lower(&rejected).unwrap_err();
    }
    for keywords in [
        json!(["ward"]),
        json!(["aura"]),
        json!(["ward", "aura"]),
        json!(["storm"]),
        json!(["rush"]),
        json!(["bane"]),
        json!(["intimidate"]),
        json!(["assail"]),
        json!(["ward", "storm"]),
    ] {
        let mut supported = tree.clone();
        supported.pointer_mut(pointer).unwrap()["keywords"] = keywords;
        lowerer.lower(&supported).unwrap();
    }
    for keywords in [
        json!(["drain"]),
        json!(["ward", "drain"]),
        json!(["quick", "aura"]),
    ] {
        let mut unsupported = tree.clone();
        unsupported.pointer_mut(pointer).unwrap()["keywords"] = keywords;
        assert!(expanded.is_valid(&unsupported));
        let failure = lowerer.lower(&unsupported).unwrap_err();
        assert_eq!(failure.code, "unsupported_construct");
        assert_eq!(failure.path, format!("{pointer}/keywords"));
    }
}

#[test]
fn a_keyword_cannot_borrow_a_current_body_obligation() {
    let mut input = current_inputs()["switch"]["after"]["authors"][0]["input"].clone();
    let ability = &mut input["tree"]["cards"]["BP01-009"]["faces"][CURRENT_FACE]["abilities"][0];
    ability.as_object_mut().unwrap().remove("evolve");
    ability["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("ability_scope");
    ability["keywords"] = json!(["quick"]);
    let lowerer = ExpandedLowerer::new().unwrap();
    for failure in [
        lowerer.check_current(&input).unwrap_err(),
        lowerer.lower_current(&input).unwrap_err(),
    ] {
        assert_eq!(failure.code, "invalid_current_source");
    }
}

#[test]
fn bp08_candidate_matches_the_handwritten_card_program() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut actual = cards["BP08-038"].card().clone();
    let expected = bp08().node;
    let [ability] = actual.abilities.as_mut_slice() else {
        panic!("one fanfare");
    };
    assert_eq!(ability.source.line, 2);
    assert_eq!(ability.source.scope, "ability_0");
    assert_eq!(ability.source.author_path, "/cards/BP08-038/abilities/0");
    ability.source = source("BP08-038");
    normalize_printed_origin(&mut ability.node.source);
    let body = ability.node.body.as_mut().unwrap();
    let [step] = body.steps.as_mut_slice() else {
        panic!("one draw");
    };
    assert_eq!(
        step.source.author_path,
        "/cards/BP08-038/abilities/0/fanfare/do/0"
    );
    step.source = source("BP08-038");
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("draw instruction");
    };
    normalize_printed_origin(&mut instruction.source);
    assert_eq!(actual, expected);
}

fn normalize_printed_origin(source: &mut AbilitySource) {
    let AbilityOrigin::Keyword { origin, .. } = &mut source.origin else {
        panic!("fanfare origin");
    };
    let AbilityOrigin::Printed { face, line } = origin.as_mut() else {
        panic!("printed origin");
    };
    *face = FaceId("base".to_owned());
    *line = Nat(1);
}

#[test]
fn unsupported_step_rejects_the_entire_candidate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"dice":{"bind":"roll"}}));
    let failure = ExpandedLowerer::new().unwrap().lower(&tree).unwrap_err();
    assert_eq!(failure.code, "unsupported_construct");
    assert_eq!(failure.path, "/cards/BP08-038/abilities/0");
}

#[test]
fn leader_life_lowering_accepts_signed_delta_and_set_but_rejects_unhandled_metadata() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([
        {"life":{"who":"self","delta":-3_i64}},
        {"life":{"who":"self","delta":0_i64}},
        {"set_life":{"who":"self","n":1_u64}}
    ]);
    lowerer.lower(&tree).unwrap();
    for metadata in [
        json!({"result":"healed"}),
        json!({"precondition":{"true":true}}),
        json!({"instead":[{}]}),
    ] {
        let mut changed = tree.clone();
        changed["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["life"]
            .as_object_mut()
            .unwrap()
            .extend(metadata.as_object().unwrap().clone());
        lowerer.lower(&changed).unwrap_err();
    }
}

#[test]
fn basic_object_actions_lower_only_known_references_and_handled_metadata() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let mut tree = expanded::tree().clone();
    for action in ["banish", "act", "stand"] {
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{(action):"this"}]);
        lowerer.lower(&tree).unwrap();
        for value in [
            json!("undefined"),
            json!({"ref":"this","result":"unhandled"}),
        ] {
            tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{(action):value}]);
            lowerer.lower(&tree).unwrap_err();
        }
    }
}

#[test]
fn bp01_118_is_one_uniform_instruction_for_all_leaders() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let [ability] = cards["BP01-118"].card().abilities.as_slice() else {
        panic!("one fanfare");
    };
    let [step] = ability.node.body.as_ref().unwrap().steps.as_slice() else {
        panic!("one damage instruction");
    };
    let Node::Instruction { instruction } = &step.node else {
        panic!("damage instruction");
    };
    assert_eq!(instruction.id, InstructionId(1));
    assert_eq!(instruction.receipt, ReceiptId(1));
    assert_eq!(
        instruction.action,
        Action::Damage {
            recipients: RecipientSet::Leaders {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None
                    },
                    text_quantifier: TextQuantifier::All,
                }
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(1))
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        }
    );
}

#[test]
fn bp01_037_fanfare_buffs_own_field_followers_except_its_source() {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP08-038"].clone();
    card["abilities"][0]["fanfare"] = json!({"do":[{"buff":{
        "on":{"who":"self","zone":"field","kind":"follower","other":true},
        "power":1_i64,"hp":1_i64
    }}]});
    tree["cards"] = json!({"BP01-037":card});
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let ability = &cards["BP01-037"].card().abilities[0].node;
    assert!(matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Fanfare,
            ..
        }
    ));
    let [step] = ability.body.as_ref().unwrap().steps.as_slice() else {
        panic!("one buff instruction");
    };
    let Node::Instruction { instruction } = &step.node else {
        panic!("buff instruction");
    };
    let Action::Modify {
        subjects,
        patch,
        expires,
        ..
    } = &instruction.action
    else {
        panic!("numeric modification");
    };
    let Selector::Difference { input, except } = subjects else {
        panic!("source exclusion");
    };
    assert!(matches!(input.as_ref(), Selector::Zone {
        players: PlayerSet { relation: PlayerRelation::SelfPlayer, .. },
        zone: ZoneKind::Field,
        predicate,
        ..
    } if matches!(predicate.as_ref(), Expr::Compute(predicate)
        if matches!(predicate.as_ref(), Predicate::And { predicates }
            if matches!(predicates.as_slice(), [Expr::Compute(kind)]
                if matches!(kind.as_ref(), Predicate::KindEquals {
                    right: Expr::Literal(CardKind::Follower), ..
                }))))));
    assert_eq!(
        except.as_ref(),
        &Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}]
        }
    );
    assert_eq!(
        patch,
        &CharacteristicPatch::Combined {
            patches: vec![
                CharacteristicPatch::Numeric {
                    stat: Stat::Attack,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1))
                    }
                },
                CharacteristicPatch::Numeric {
                    stat: Stat::Life,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1))
                    }
                },
            ]
        }
    );
    assert!(matches!(expires.end, DurationEnd::NoTemporalExpiry {}));

    tree["cards"]["BP01-037"]["abilities"][0]["fanfare"]["do"][0]["buff"]["on"]
        .as_object_mut()
        .unwrap()
        .remove("other");
    let unrestricted = lowerer.lower(&tree).unwrap();
    let unrestricted_step = &unrestricted["BP01-037"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps[0];
    assert!(
        matches!(&unrestricted_step.node, Node::Instruction { instruction: unfiltered }
        if matches!(unfiltered.action, Action::Modify { subjects: Selector::Zone { .. }, .. }))
    );
}

#[test]
fn numeric_duration_and_continuous_preserve_distinct_semantics() {
    let mut tree = expanded::tree().clone();
    let steps = &mut tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"];
    *steps = json!([
        {"set_stat":{"on":"this","hp":1_i64,"until":"next_opp_turn"}},
        {"buff":{"on":"this","power":-1_i64,"until":"this_and_next_opp_turn"}},
        {"apply":{"on":"this","effect":{"stat":{"hp":2_i64}},"until":"turn"}},
        {"apply":{"on":{"who":"self","zone":"field"},"effect":{"stat":{"power":1_i64}}}}
    ]);
    let second = tree["cards"]["BP08-038"]["abilities"][0].clone();
    tree["cards"]["BP08-038"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let cards = ExpandedLowerer::new().unwrap().lower(&tree).unwrap();
    let body = cards["BP08-038"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let actions = body
        .steps
        .iter()
        .map(|step| match &step.node {
            Node::Instruction { instruction } => &instruction.action,
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
                panic!("instruction")
            }
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        actions[0],
        Action::Modify {
            patch: CharacteristicPatch::Numeric {
                change: NumericChange::Set { .. },
                ..
            },
            expires: Duration {
                end: DurationEnd::NextTurnEnd { .. },
                ..
            },
            ..
        }
    ));
    assert!(
        matches!(actions[1], Action::Modify { expires: Duration { end: DurationEnd::CurrentTurnEnd {}, intervals, .. }, .. } if matches!(intervals.as_slice(), [Interval::DuringNextTurn { .. }]))
    );
    assert!(
        matches!(actions[2], Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Continuous { layer: ContinuousLayer::Numeric, .. }))
    );
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["set_stat"]["hp"] = json!("X");
    assert_eq!(
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

#[test]
fn search_groups_partition_and_successful_ex_discount_are_scoped() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"] = json!([
        {"search":{"groups":[
            {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u64},
            {"sel":{"who":"self","zone":"deck","kind":"spell"},"upto":1_u64}
        ],"distinct_by":"name","to":{"order":"printed","partition":[{"zone":"hand","n":1_u64},{"zone":"cemetery","n":1_u64}]}}},
        {"search":{"sel":{"who":"self","zone":"deck","name":"コスモスファング"},"n":1_u64,"to":"ex","result":"searched"}},
        {"apply":{"on":{"follow":"searched"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"delta":-2_i64}},"until":"turn"}},
        {"look":{"top":{"who":"self","n":3_u64},"bind":"seen"}},
        {"move":{"ref":{"in":"seen"},"to":"ex","result":"moved"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}}
    ]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let body = cards["BP03-002"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Instruction { instruction: first } = &body.steps[0].node else {
        panic!("search");
    };
    let Action::Search {
        groups: Some(groups),
        destination: Some(SearchDestination::Partition { destinations }),
        ..
    } = &first.action
    else {
        panic!("partition");
    };
    assert_eq!(
        groups.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(2))
        }
    );
    assert_eq!(destinations.len(), 2);
    let Node::Instruction {
        instruction: searched,
    } = &body.steps[1].node
    else {
        panic!("search");
    };
    let Node::Instruction {
        instruction: installed,
    } = &body.steps[2].node
    else {
        panic!("installation");
    };
    assert!(
        matches!(&installed.action, Action::InstallEffect { subjects: InstallationSubjects::Objects { objects: Selector::MoveAfter { receipt: MovementReceipt::Search { receipt }, .. }, .. }, .. } if *receipt == ReceiptReference::new(searched.receipt))
    );
    let Node::Instruction {
        instruction: ordered,
    } = &body.steps[5].node
    else {
        panic!("order");
    };
    assert!(
        matches!(&ordered.action, Action::Order { cards: Selector::Difference { except, .. }, .. } if matches!(except.as_ref(), Selector::ReceiptMembers { receipt: ObjectReceipt::Movement { receipt: MovementReceipt::Move { .. } }, projection: MemberProjection::Before }))
    );
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"][1]["search"]["to"] = json!("hand");
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn activated_payment_order_and_trigger_limit_remain_separate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-067"]["abilities"][0]["auto"]["per_turn"] = json!(1_u64);
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"self_to_cemetery":true}));
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let ability = &cards["BP01-041"].card().abilities[0].node;
    assert_eq!(ability.kind, AbilityKind::Activated);
    assert!(matches!(
        ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    assert!(ability.trigger.is_none());
    let CostSpec::Ordered { steps } = &ability.play.as_ref().unwrap().costs else {
        panic!("ordered payment");
    };
    assert!(matches!(
        steps[0].instruction.action,
        Action::Resource {
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(3)),
            ..
        }
    ));
    assert!(matches!(
        steps[1].instruction.action,
        Action::Posture {
            posture: Posture::Acted,
            ..
        }
    ));
    assert!(matches!(&steps[2].instruction.action, Action::Move {
        objects: Selector::Objects { objects },
        movement: MoveSpec { to: MoveDestination::Owners { zone: OwnerZone::Cemetery }, position: None, order: None, initial_posture: None, entry_effects },
        reason: MoveReason::Put,
    } if objects == &[ObjectOperand::SourceHolder {}] && entry_effects.is_empty()));
    assert_eq!(
        cards["BP08-067"].card().abilities[0]
            .node
            .trigger
            .as_ref()
            .unwrap()
            .limit,
        Some(LimitSpec::once_per_turn())
    );
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["per_turn"] = json!(2_u64);
    assert_eq!(
        lowerer.lower(&tree).unwrap()["BP01-041"].card().abilities[0]
            .node
            .play
            .as_ref()
            .unwrap()
            .limit,
        Some(LimitSpec::activated_per_turn(Nat(2)))
    );
}

#[test]
fn printed_spell_cost_targets_distribution_and_choice_preserve_play_scopes() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let mut tree = expanded::spell_tree().clone();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
    let initial_cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let initial_ability = &initial_cards["SD03-005"].card().abilities[0].node;
    assert_eq!(initial_ability.kind, AbilityKind::Spell);
    assert_eq!(initial_ability.active_zones, vec![ZoneKind::Resolution]);
    assert!(matches!(
        initial_ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    let body = "/cards/SD03-005/abilities/0/spell";
    *tree.pointer_mut(body).unwrap() = json!({
        "targets":[{"bind":"t","from":{"who":"opp","zone":"field"},"upto":2_u64}],
        "do":[{"damage":{"to":"t","n":3_u64,"distribute":true}}]
    });
    let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let ability = &cards["SD03-005"].card().abilities[0].node;
    assert_play_distribution(ability);
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["kind"] = json!("follower");
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["trait"] = json!("兵士");
    let filtered = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    assert_target_characteristics(&filtered["SD03-005"]);
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["power_max"] = json!(3_u64);
    lowerer.lower_with_printed_costs(&tree, &costs).unwrap_err();
    *tree.pointer_mut(body).unwrap() = json!({"targets":[{"bind":"mode_t","from":{"who":"opp","zone":"field"},"n":1_u64,"option":1_u64}], "do":[{"choice":{"n":1_u64,"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]}}]});
    let choices = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let program = choices["SD03-005"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Control {
        control:
            Control::Mode {
                modes,
                chosen,
                choice_kind,
                ..
            },
    } = &program.steps[0].node
    else {
        panic!("choice keyword");
    };
    assert_eq!(*choice_kind, ModeChoiceKind::ChoiceKeyword);
    assert!(matches!(
        chosen.kind,
        ModeDecisionKind::PlayParameter { .. }
    ));
    let ChoiceDomain::Objects { binding, .. } = modes[0].parameters[0].domain else {
        panic!("mode target");
    };
    assert_eq!(binding.key().0, modes[0].body.scope);
    assert_eq!(modes[1].parameters, []);
    assert_ne!(modes[0].body.scope, modes[1].body.scope);
    assert_ne!(modes[0].body.scope, program.scope);
    assert!(
        modes[0].body.steps[0]
            .source
            .author_path
            .ends_with("/spell/do/0/choice/options/0/0")
    );
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(3_u64);
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, &costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    assert_choice_extensions(&lowerer, tree, &costs, body);
}

fn assert_choice_extensions(
    lowerer: &ExpandedLowerer,
    mut tree: Value,
    costs: &BTreeMap<String, Nat>,
    body: &str,
) {
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(1_u64);
    tree.pointer_mut(body).unwrap()["do"][0]["choice"]["exclude_chosen"] = json!("this_turn");
    let excluded = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        &excluded["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps[0]
            .node,
        Node::Control {
            control: Control::Mode {
                chosen: ModeDecision {
                    domain: ChoiceDomain::Modes {
                        exclude_used: Some(ModeExclusion::CurrentTurnAbilityInstance),
                        ..
                    },
                    ..
                },
                ..
            }
        }
    ));
    tree.pointer_mut(body).unwrap()["do"][0]["choice"] = json!({"upto":2_u64,"instead":[{"if":{"spellchain":3_u64},"upto":1_u64}],"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]});
    let conditional = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        &conditional["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps[0]
            .node,
        Node::Control {
            control: Control::Mode {
                count: Expr::Compute(_),
                ..
            }
        }
    ));
    tree.pointer_mut(body).unwrap()["do"][0]["choice"]["instead"][0] =
        json!({"if":{"necro":3_u64},"n":2_u64});
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    let steps = &mut tree.pointer_mut(body).unwrap()["do"];
    steps[0]["choice"] = json!({"n":1_u64,"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]});
    steps
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"draw":{"n":1_u64}}));
    let surrounded = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        surrounded["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps
            .as_slice(),
        [
            Located {
                node: Node::Instruction { .. },
                ..
            },
            Located {
                node: Node::Control {
                    control: Control::Mode { .. }
                },
                ..
            }
        ]
    ));
    let second = tree.pointer(&format!("{body}/do/1")).unwrap().clone();
    tree.pointer_mut(&format!("{body}/do"))
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(second);
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

fn assert_play_distribution(ability: &Ability) {
    let parameters = &ability.play.as_ref().unwrap().parameters;
    assert_eq!(parameters.len(), 2);
    let ChoiceDomain::Objects {
        binding: target,
        constraint,
        ..
    } = &parameters[0].domain
    else {
        panic!("targets");
    };
    assert_eq!(
        constraint.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        }
    );
    let ChoiceDomain::Distribution {
        recipients,
        total,
        minimum,
        binding,
    } = &parameters[1].domain
    else {
        panic!("distribution");
    };
    assert_eq!(
        recipients,
        &RecipientSet::Objects {
            objects: Selector::Bound { reference: *target }
        }
    );
    assert_eq!(total, &Expr::Literal(Nat(3)));
    assert_eq!(minimum, &Nat(1));
    let Node::Instruction { instruction } = &ability.body.as_ref().unwrap().steps[0].node else {
        panic!("damage");
    };
    assert!(
        matches!(instruction.action, Action::Damage { amount: DamageAmount::Allocation { binding: actual }, .. } if actual == *binding)
    );
}

#[test]
#[expect(
    clippy::default_numeric_fallback,
    reason = "Synthetic author JSON contains literal cost amounts."
)]
fn own_play_costs_preserve_explicit_intervals_and_reject_payment_shapes() {
    let mut tree = expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let count = json!({"count":{"who":"self","zone":"cemetery","kind":"spell"}});
    let threshold = |op, right| json!({"cmp":{"left":count,"op":op,"right":right}});
    entry["play"] = json!({"cost_delta":[
        {"if":threshold("ge",15),"delta":-9},
        {"if":{"all":[threshold("ge",10),threshold("lt",15)]},"delta":-6},
        {"if":{"all":[threshold("ge",5),threshold("lt",10)]},"delta":-3},-1,-1],"cost_set":7});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"synthetic":{"meta":meta,"abilities":[entry]}});
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let card = cards["synthetic"].card();
    assert_eq!(card.abilities, []);
    assert_eq!(card.card_play_costs[0].node.modifiers.len(), 6);
    assert!(
        card.card_play_costs[0].node.modifiers[1]
            .source
            .author_path
            .ends_with("/play/cost_delta/1")
    );
    let located = Located {
        source: source("synthetic"),
        node: card.clone(),
    };
    let requirements = Requirements::derive(&located);
    assert!(requirements.required.iter().any(|r| {
        r.capability.code == "card.play-cost"
            && r.locations
                .iter()
                .all(|location| location.ir_path.contains("card_play_costs"))
    }));
    tree["cards"]["synthetic"]["abilities"][0]["play"]["optional_cost"] = json!({"pp":1});
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().path,
        "/cards/synthetic/abilities/0"
    );
}

fn assert_target_characteristics(filtered: &WellFormedCardProgram) {
    let ChoiceDomain::Objects { constraint, .. } = &filtered.card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .parameters[0]
        .domain
    else {
        panic!("targets");
    };
    let Selector::Zone { predicate, .. } = &constraint.groups[0].selector else {
        panic!("field");
    };
    assert!(
        matches!(predicate.as_ref(), Expr::Compute(operation) if matches!(operation.as_ref(), Predicate::And { predicates } if matches!(predicates.as_slice(), [Expr::Compute(kind), Expr::Compute(traits)] if matches!(kind.as_ref(), Predicate::KindEquals { right: Expr::Literal(CardKind::Follower), .. }) && matches!(traits.as_ref(), Predicate::HasTrait { trait_id, .. } if trait_id.0 == "兵士"))))
    );
    assert_eq!(
        filtered.card().abilities[0].source.author_path,
        "/cards/SD03-005/abilities/0"
    );
}

#[test]
fn leader_targets_and_leader_follower_targets_keep_recipient_bindings() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    for from in [
        json!({"leader":"opp"}),
        json!({"leader":"self"}),
        json!({"leaders":"all"}),
        json!({"who":"opp","kind":"follower","leader_or":true}),
    ] {
        let mut tree = expanded::spell_tree().clone();
        *tree
            .pointer_mut("/cards/SD03-005/abilities/0/spell")
            .unwrap() = json!({
            "targets":[{"bind":"victim","from":from,"n":1_u64}],
            "do":[{"damage":{"to":"victim","n":3_u64}}]
        });
        let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
        let ability = &cards["SD03-005"].card().abilities[0].node;
        assert!(matches!(
            ability.play.as_ref().unwrap().parameters[0].domain,
            ChoiceDomain::Recipients { .. }
        ));
        let mut distinct = tree.clone();
        distinct["cards"]["SD03-005"]["abilities"][0]["spell"]["targets"][0]["distinct_by"] =
            json!("name");
        assert!(
            lowerer.lower_with_printed_costs(&distinct, &costs).is_err(),
            "leaders have no card-name distinctness contract"
        );
        tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] = json!([{"destroy":"victim"}]);
        assert!(
            lowerer.lower_with_printed_costs(&tree, &costs).is_err(),
            "recipient bindings cannot become object selectors"
        );
    }
}

#[test]
fn activated_zones_preserve_ex_and_reject_unimplemented_combinations() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for zone in [json!("field"), json!(["field"]), json!("ex"), json!(["ex"])] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["zone"] = zone.clone();
        let cards = lowerer.lower(&tree).unwrap();
        let expected = if zone == json!("ex") || zone == json!(["ex"]) {
            ZoneKind::Ex
        } else {
            ZoneKind::Field
        };
        assert_eq!(
            cards["BP01-041"].card().abilities[0].node.active_zones,
            [expected]
        );
    }
    for zone in [
        json!("hand"),
        json!(["field", "ex"]),
        json!([]),
        json!(["ex", "ex"]),
    ] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["zone"] = zone;
        lowerer.lower(&tree).unwrap_err();
    }
}

#[test]
fn repeated_named_existence_queries_keep_independent_candidate_scopes() {
    let mut tree = expanded::spell_tree().clone();
    let condition = json!({"exists":{"who":"self","zone":"field","name":"unit-follower"}});
    *tree
        .pointer_mut("/cards/SD03-005/abilities/0/spell")
        .unwrap() = json!({
        "do":[
            {"if":{"cond":condition,"then":[{"draw":{"who":"self","n":1_u8}}]}},
            {"if":{"cond":condition,"then":[{"draw":{"who":"self","n":1_u8}}]}}
        ]
    });
    ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]))
        .unwrap();
}

#[test]
fn resolution_leader_choices_keep_recipient_bindings() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for from in [
        json!({"leader":"opp"}),
        json!({"who":"opp","kind":"follower","leader_or":true}),
    ] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([
            {"choose":{"bind":"victim","from":from,"n":1_u64}},
            {"damage":{"to":"victim","n":1_u64}}
        ]);
        let cards = lowerer.lower(&tree).unwrap();
        let steps = &cards["BP08-038"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps;
        assert!(matches!(&steps[0].node, Node::Decision { decision }
            if matches!(decision.domain, ChoiceDomain::Recipients { .. })
            && matches!(decision.kind, DecisionKind::ResolutionChoice { .. })));
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][1] = json!({"destroy":"victim"});
        lowerer.lower(&tree).unwrap_err();
    }
}
