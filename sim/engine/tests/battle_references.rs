//! Printing identities are explicit, complete and bound to exact snapshot bytes.

#![allow(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Fixture assertions can panic and pattern checks only handle the expected variants."
)]

use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_battle_contract::{CardRef, Nat};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ProjectionError, ReferenceInput};
use sve_engine::catalog::{Catalog, EngineIdentityInput, FaceIdentity};
use sve_engine::game::player::observation::{CardFace, WordingField, WordingSource};

#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

struct Fixture {
    catalog: Catalog,
    snapshot: String,
    keywords: String,
    documents: Vec<(String, String)>,
    input: ReferenceInput,
}

fn identity(number: &str, native_face: u64, printing: &str, face: u32) -> PrintingFaceIdentity {
    PrintingFaceIdentity {
        card_no: number.into(),
        native_face,
        card: CardRef {
            printing_id: printing.into(),
            face: Nat::from(face),
        },
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic inputs must fail fixture creation."
)]
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let face = json!({"name":"synthetic","title":"synthetic-title","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]});
        let snapshot = [
            json!({"number":"nativeⓈ","faces":[face, face]}),
            json!({"number":"nativeⓈEN","faces":[face]}),
        ].iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        let keywords = json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string();
        let documents = vec![("synthetic.yaml".into(), json!({"format":1_u8,"kind":"fixture_cards","cards":{
            "nativeⓈ":{"status":"complete"},
            "nativeⓈEN":{"status":"complete"}
        }}).to_string())];
        let rules = rule_fixtures::rules(
            &snapshot,
            json!([rule_fixtures::title("synthetic", "nativeⓈ", json!([]))]),
            json!([]),
        );
        let catalog = rule_fixtures::catalog(
            &snapshot, &keywords, &documents, &rules.to_string(),
        ).unwrap();
        let input = ReferenceInput {
            rules: None,
wording: None,
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces: vec![
                identity("nativeⓈ", 0, "printing-y", 1),
                identity("nativeⓈ", 1, "printing-y", 0),
                identity("nativeⓈEN", 0, "printing-x", 0),
            ],
        };
        Fixture { catalog, snapshot, keywords, documents, input }
    })
}

fn bind(input: ReferenceInput) -> Result<BattleReferences, ProjectionError> {
    BattleReferences::new(&fixture().catalog, input)
}

#[test]
fn explicit_printings_and_face_coordinates_are_preserved_without_number_inference() {
    let mut input = fixture().input.clone();
    input.faces.reverse();
    let references = bind(input).unwrap();
    for identity in &fixture().input.faces {
        assert_eq!(
            references
                .resolve(
                    &fixture().catalog,
                    &CardFace {
                        card: identity.card_no.clone(),
                        face: identity.native_face
                    }
                )
                .unwrap(),
            identity.card
        );
    }
    let debug = format!("{references:?}");
    assert!(!debug.contains("nativeⓈ"));
    assert!(!debug.contains("printing-y"));
}

#[test]
fn references_require_a_matching_bound_region_and_exact_snapshot() {
    let source = fixture();
    let unbound =
        Catalog::from_fixture_documents(&source.snapshot, &source.keywords, &source.documents)
            .unwrap();
    assert_eq!(
        BattleReferences::new(&unbound, source.input.clone()).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_region = source.input.clone();
    wrong_region.region = "en".into();
    assert_eq!(
        bind(wrong_region).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_snapshot = source.input.clone();
    wrong_snapshot.snapshot_sha256 = "0".repeat(64);
    assert_eq!(
        bind(wrong_snapshot).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn resolved_en_background_accepts_explicit_permanent_printings() {
    let source = fixture();
    let mut rules = rule_fixtures::rules(
        &source.snapshot,
        json!([{"title_code":"synthetic","anchor":{"kind":"face-id","region":"en","face_id":"face-0"},"capabilities":[],"evidence":["synthetic"]}]),
        json!([]),
    );
    for (object, key, value) in [
        ("scope", "region", json!("en")),
        ("input", "kind", json!("resolved")),
    ] {
        *rules.get_mut(object).unwrap().get_mut(key).unwrap() = value;
    }
    let identity = EngineIdentityInput::Resolved {
        region: "en".into(),
        snapshot_sha256: source.input.snapshot_sha256.clone(),
        faces: source
            .input
            .faces
            .iter()
            .enumerate()
            .map(|(index, face)| FaceIdentity {
                card_no: face.card_no.clone(),
                face_ordinal: usize::try_from(face.native_face).unwrap(),
                face_id: format!("face-{index}"),
                rules_name_id: "synthetic-name".into(),
                title_code: Some("synthetic".into()),
            })
            .collect(),
    };
    let catalog = Catalog::from_fixture_documents_with_rules(
        &source.snapshot,
        &source.keywords,
        &source.documents,
        &rules.to_string(),
        &identity,
    )
    .unwrap();
    let mut input = source.input.clone();
    input.region = "en".into();
    let references = BattleReferences::new(&catalog, input).unwrap();
    assert_eq!(
        references
            .resolve(
                &catalog,
                &CardFace {
                    card: "nativeⓈEN".into(),
                    face: 0
                }
            )
            .unwrap(),
        CardRef {
            printing_id: "printing-x".into(),
            face: Nat::ZERO
        }
    );
    assert_eq!(
        bind(source.input.clone())
            .unwrap()
            .resolve(
                &catalog,
                &CardFace {
                    card: "nativeⓈEN".into(),
                    face: 0,
                }
            )
            .unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn lookup_rechecks_the_catalogue_even_when_native_coordinates_are_identical() {
    let source = fixture();
    let snapshot = format!("{}\n", source.snapshot);
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title("synthetic", "nativeⓈ", json!([]))]),
        json!([]),
    );
    let other = rule_fixtures::catalog(
        &snapshot,
        &source.keywords,
        &source.documents,
        &rules.to_string(),
    )
    .unwrap();
    let references = bind(source.input.clone()).unwrap();
    let face = CardFace {
        card: "nativeⓈ".into(),
        face: 0,
    };
    assert_eq!(
        references.resolve(&other, &face).unwrap_err(),
        ProjectionError::Unavailable
    );
    assert_eq!(
        references.resolve(&source.catalog, &face).unwrap(),
        source.input.faces.first().unwrap().card
    );
}

#[test]
fn partial_duplicate_unknown_or_out_of_range_faces_are_refused() {
    let mut missing = fixture().input.clone();
    missing.faces.pop().unwrap();
    assert_eq!(bind(missing).unwrap_err(), ProjectionError::Unavailable);
    let mut duplicate = fixture().input.clone();
    duplicate
        .faces
        .push(duplicate.faces.first().unwrap().clone());
    assert_eq!(bind(duplicate).unwrap_err(), ProjectionError::Unavailable);
    let mut unknown_card = fixture().input.clone();
    unknown_card.faces.first_mut().unwrap().card_no = "unknown-native".into();
    assert_eq!(
        bind(unknown_card).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut unknown_native_face = fixture().input.clone();
    unknown_native_face.faces.first_mut().unwrap().native_face = u64::MAX;
    assert_eq!(
        bind(unknown_native_face).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut unknown_printed_face = fixture().input.clone();
    unknown_printed_face.faces.first_mut().unwrap().card.face = Nat::from(2_u32);
    assert_eq!(
        bind(unknown_printed_face).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn each_native_printing_has_one_distinct_permanent_printing_and_face_bijection() {
    let mut inconsistent = fixture().input.clone();
    inconsistent.faces.first_mut().unwrap().card.printing_id = "different-printing".into();
    assert_eq!(
        bind(inconsistent).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut aliased = fixture().input.clone();
    aliased.faces.last_mut().unwrap().card.printing_id = "printing-y".into();
    aliased.faces.rotate_right(1);
    assert_eq!(bind(aliased).unwrap_err(), ProjectionError::Unavailable);
    let mut duplicate_face = fixture().input.clone();
    duplicate_face.faces.first_mut().unwrap().card.face = Nat::ZERO;
    assert_eq!(
        bind(duplicate_face).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut empty_printing = fixture().input.clone();
    empty_printing
        .faces
        .first_mut()
        .unwrap()
        .card
        .printing_id
        .clear();
    assert_eq!(
        bind(empty_printing).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn unknown_lookup_never_falls_back_to_a_native_number_or_default_face() {
    let references = bind(fixture().input.clone()).unwrap();
    for face in [
        CardFace {
            card: "nativeⓈen".into(),
            face: 0,
        },
        CardFace {
            card: "nativeⓈ".into(),
            face: u64::MAX,
        },
    ] {
        assert_eq!(
            references.resolve(&fixture().catalog, &face).unwrap_err(),
            ProjectionError::Unavailable
        );
    }
}

#[test]
fn trusted_reference_input_serialization_is_closed() {
    let input = fixture().input.clone();
    let value = serde_json::to_value(&input).unwrap();
    assert_eq!(
        serde_json::from_value::<ReferenceInput>(value.clone()).unwrap(),
        input
    );
    let mut unknown = value.as_object().unwrap().clone();
    unknown.insert("fallback".into(), json!(true));
    serde_json::from_value::<ReferenceInput>(Value::Object(unknown)).unwrap_err();
}

#[expect(
    clippy::unwrap_used,
    reason = "Invalid synthetic wording inputs must fail fixture creation."
)]
fn wording_input() -> Value {
    let mut input = serde_json::to_value(&fixture().input).unwrap();
    input.as_object_mut().unwrap().insert("wording".into(), json!({
        "manifest_sha256": format!("sha256:{}", "a".repeat(64)),
        "faces": [
            {"card_no":"nativeⓈ","native_face":0_u64,"face_id":"face-front","revisions":[
                {"revision_id":"rev-old","fields":[{"kind":"effect"},{"kind":"section","ordinal":0_u64}]},
                {"revision_id":"rev-new","fields":[{"kind":"effect"}]}
            ]},
            {"card_no":"nativeⓈ","native_face":1_u64,"face_id":"face-back","revisions":[
                {"revision_id":"rev-back","fields":[{"kind":"section","ordinal":2_u64}]}
            ]},
            {"card_no":"nativeⓈEN","native_face":0_u64,"face_id":"face-other","revisions":[]}
        ]
    }));
    input
}

#[test]
fn wording_input_retains_the_public_manifest_and_complete_face_closure() {
    let input = wording_input();
    let parsed: ReferenceInput = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), input);
    let references = bind(parsed).unwrap();
    assert_eq!(
        references
            .resolve(
                &fixture().catalog,
                &CardFace {
                    card: "nativeⓈ".into(),
                    face: 0
                }
            )
            .unwrap(),
        CardRef {
            printing_id: "printing-y".into(),
            face: Nat::from(1_u32)
        }
    );
    let manifest = format!("sha256:{}", "a".repeat(64));
    assert_eq!(references.manifest_sha256(), Some(manifest.as_str()));
    assert_eq!(
        bind(fixture().input.clone()).unwrap().manifest_sha256(),
        None
    );
}

#[test]
fn wording_lookup_requires_the_exact_face_revision_and_original_field() {
    let references = bind(serde_json::from_value(wording_input()).unwrap()).unwrap();
    for (native_face, face_id, revision, field, permanent_face) in [
        (0, "face-front", "rev-old", WordingField::Effect, 1_u32),
        (
            0,
            "face-front",
            "rev-old",
            WordingField::Section { ordinal: 0 },
            1,
        ),
        (0, "face-front", "rev-new", WordingField::Effect, 1),
        (
            1,
            "face-back",
            "rev-back",
            WordingField::Section { ordinal: 2 },
            0,
        ),
    ] {
        assert_eq!(
            references
                .validate_wording(
                    &fixture().catalog,
                    &CardFace {
                        card: "nativeⓈ".into(),
                        face: native_face
                    },
                    &WordingSource {
                        face_id: face_id.into(),
                        revision_id: revision.into(),
                        field
                    }
                )
                .unwrap(),
            CardRef {
                printing_id: "printing-y".into(),
                face: Nat::from(permanent_face)
            }
        );
    }
    for (native_face, face_id, revision, field) in [
        (0, "face-back", "rev-old", WordingField::Effect),
        (0, "face-front", "rev-back", WordingField::Effect),
        (0, "face-front", "unknown", WordingField::Effect),
        (
            0,
            "face-front",
            "rev-new",
            WordingField::Section { ordinal: 0 },
        ),
        (
            0,
            "face-front",
            "rev-old",
            WordingField::Section { ordinal: 1 },
        ),
        (1, "face-back", "rev-back", WordingField::Effect),
        (99, "face-front", "rev-old", WordingField::Effect),
    ] {
        assert_eq!(
            references
                .validate_wording(
                    &fixture().catalog,
                    &CardFace {
                        card: "nativeⓈ".into(),
                        face: native_face
                    },
                    &WordingSource {
                        face_id: face_id.into(),
                        revision_id: revision.into(),
                        field
                    }
                )
                .unwrap_err(),
            ProjectionError::Unavailable
        );
    }
}

#[test]
fn wording_lookup_cannot_replace_missing_evidence_or_cross_catalogues() {
    let face = CardFace {
        card: "nativeⓈ".into(),
        face: 0,
    };
    let mut source = WordingSource {
        face_id: "face-front".into(),
        revision_id: "rev-old".into(),
        field: WordingField::Effect,
    };
    let missing = bind(fixture().input.clone()).unwrap();
    assert_eq!(
        missing
            .validate_wording(&fixture().catalog, &face, &source)
            .unwrap_err(),
        ProjectionError::Unavailable
    );
    let references = bind(serde_json::from_value(wording_input()).unwrap()).unwrap();
    let foreign = Catalog::from_fixture_documents(
        &fixture().snapshot,
        &fixture().keywords,
        &fixture().documents,
    )
    .unwrap();
    assert_eq!(
        references
            .validate_wording(&foreign, &face, &source)
            .unwrap_err(),
        ProjectionError::Unavailable
    );
    source.field = WordingField::Section { ordinal: u64::MAX };
    assert_eq!(
        references
            .validate_wording(&fixture().catalog, &face, &source)
            .unwrap_err(),
        ProjectionError::Range
    );
}

#[test]
fn wording_bindings_refuse_partial_aliasing_or_contradictory_owners() {
    for (pointer, value) in [
        ("/wording/manifest_sha256", json!("a".repeat(64))),
        (
            "/wording/manifest_sha256",
            json!(format!("sha256:{}", "A".repeat(64))),
        ),
        ("/wording/faces", json!([])),
        ("/wording/faces/0/native_face", json!(99_u32)),
        ("/wording/faces/0/face_id", json!("")),
        ("/wording/faces/1/face_id", json!("face-front")),
        ("/wording/faces/1/native_face", json!(0_u32)),
        ("/wording/faces/1/revisions/0/revision_id", json!("rev-old")),
        ("/wording/faces/0/revisions/1/revision_id", json!("rev-old")),
        ("/wording/faces/0/revisions/0/revision_id", json!("")),
        (
            "/wording/faces/0/revisions/0/fields",
            json!([{"kind":"effect"},{"kind":"effect"}]),
        ),
    ] {
        let mut input = wording_input();
        *input.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            bind(serde_json::from_value(input).unwrap()).unwrap_err(),
            ProjectionError::Unavailable,
            "{pointer}"
        );
    }
}

#[test]
fn trusted_wording_input_is_closed_at_every_level() {
    for pointer in [
        "/wording",
        "/wording/faces/0",
        "/wording/faces/0/revisions/0",
        "/wording/faces/0/revisions/0/fields/0",
    ] {
        let mut input = wording_input();
        input
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        serde_json::from_value::<ReferenceInput>(input).unwrap_err();
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "Malformed synthetic rule inputs fail fixture creation."
)]
fn rule_input() -> Value {
    let mut input = wording_input();
    input.as_object_mut().unwrap().insert("rules".into(), json!({
        "manifest_sha256":format!("sha256:{}","a".repeat(64)),
        "entries":[
            {"kind":"cr","id":"1.2","revision":"cr-version"},
            {"kind":"cr","id":"3.1","revision":"cr-version"},
            {"kind":"qa","id":"answer-parent","revision":"qa-old"},
            {"kind":"qa","id":"answer-parent","revision":"qa-new"},
            {"kind":"ruling","id":"ruling-parent","revision":"ruling-old","strength":"generalized"},
            {"kind":"ruling","id":"ruling-parent","revision":"ruling-new","strength":"inferred"}
        ]
    }));
    input
}

#[test]
fn rule_locations_roundtrip_and_use_the_callers_explicit_strength() {
    use sve_battle_contract::{RuleRef, RuleSource, RuleStrength};
    let input = rule_input();
    let parsed: ReferenceInput = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), input);
    let references = bind(parsed).unwrap();
    for (kind, id, revision) in [
        (RuleSource::Cr, "1.2", "cr-version"),
        (RuleSource::Cr, "3.1", "cr-version"),
        (RuleSource::Qa, "answer-parent", "qa-old"),
        (RuleSource::Qa, "answer-parent", "qa-new"),
    ] {
        for strength in [
            RuleStrength::Official,
            RuleStrength::Generalized,
            RuleStrength::Inferred,
            RuleStrength::Undecided,
        ] {
            let reference = RuleRef {
                kind,
                id: id.into(),
                revision: revision.into(),
                strength,
            };
            assert_eq!(
                references.validate_rule(&fixture().catalog, &reference),
                Ok(reference.clone())
            );
        }
    }
    for (revision, strength) in [
        ("ruling-old", RuleStrength::Generalized),
        ("ruling-new", RuleStrength::Inferred),
    ] {
        let reference = RuleRef {
            kind: RuleSource::Ruling,
            id: "ruling-parent".into(),
            revision: revision.into(),
            strength,
        };
        assert_eq!(
            references.validate_rule(&fixture().catalog, &reference),
            Ok(reference.clone())
        );
    }
}

#[test]
fn rule_locations_reject_guessed_ids_versions_strengths_and_missing_closures() {
    use sve_battle_contract::{RuleRef, RuleSource, RuleStrength};
    let references = bind(serde_json::from_value(rule_input()).unwrap()).unwrap();
    for (kind, id, revision, strength) in [
        (
            RuleSource::Cr,
            "clause-row-id",
            "cr-version",
            RuleStrength::Official,
        ),
        (
            RuleSource::Cr,
            "1.2",
            "human-version-2.0",
            RuleStrength::Official,
        ),
        (RuleSource::Cr, "1.2", "qa-new", RuleStrength::Official),
        (RuleSource::Qa, "00042", "qa-new", RuleStrength::Official),
        (
            RuleSource::Qa,
            "other-answer",
            "qa-new",
            RuleStrength::Official,
        ),
        (RuleSource::Qa, "answer-parent", "1", RuleStrength::Official),
        (
            RuleSource::Ruling,
            "ruling-parent",
            "ruling-old",
            RuleStrength::Inferred,
        ),
        (
            RuleSource::Ruling,
            "ruling-parent",
            "ruling-new",
            RuleStrength::Official,
        ),
    ] {
        let reference = RuleRef {
            kind,
            id: id.into(),
            revision: revision.into(),
            strength,
        };
        assert_eq!(
            references.validate_rule(&fixture().catalog, &reference),
            Err(ProjectionError::Unavailable)
        );
    }
    let reference = RuleRef {
        kind: RuleSource::Cr,
        id: "1.2".into(),
        revision: "cr-version".into(),
        strength: RuleStrength::Inferred,
    };
    assert_eq!(
        bind(fixture().input.clone())
            .unwrap()
            .validate_rule(&fixture().catalog, &reference),
        Err(ProjectionError::Unavailable)
    );
    let unbound = Catalog::from_fixture_documents(
        &fixture().snapshot,
        &fixture().keywords,
        &fixture().documents,
    )
    .unwrap();
    assert_eq!(
        references.validate_rule(&unbound, &reference),
        Err(ProjectionError::Unavailable)
    );
    let mut input = rule_input();
    input["wording"] = Value::Null;
    assert_eq!(
        bind(serde_json::from_value(input).unwrap())
            .unwrap()
            .manifest_sha256(),
        Some(format!("sha256:{}", "a".repeat(64)).as_str())
    );
}

#[test]
fn rule_identity_graph_rejects_ambiguity_and_cross_manifest_claims() {
    for (path, value) in [
        (
            "/rules/manifest_sha256",
            json!(format!("sha256:{}", "b".repeat(64))),
        ),
        ("/rules/manifest_sha256", json!("unbound")),
        ("/rules/entries/0/id", json!("")),
        ("/rules/entries/0/revision", json!("")),
        ("/rules/entries/1/id", json!("1.2")),
        ("/rules/entries/3/revision", json!("qa-old")),
        ("/rules/entries/3/id", json!("other-parent")),
        ("/rules/entries/5/id", json!("other-ruling")),
    ] {
        let mut input = rule_input();
        if path.ends_with("/id") && value == json!("other-parent") {
            input["rules"]["entries"][3]["revision"] = json!("qa-old");
        }
        if path.ends_with("/id") && value == json!("other-ruling") {
            input["rules"]["entries"][5]["revision"] = json!("ruling-old");
        }
        *input.pointer_mut(path).unwrap() = value;
        assert_eq!(
            bind(serde_json::from_value(input).unwrap()).unwrap_err(),
            ProjectionError::Unavailable
        );
    }
}

#[test]
fn rule_identity_input_is_closed_and_never_supplies_a_default_strength() {
    for (path, key, value) in [
        ("/rules", "extra", json!(true)),
        ("/rules/entries/0", "strength", json!("official")),
        ("/rules/entries/2", "extra", json!(true)),
        ("/rules/entries/4", "extra", json!(true)),
    ] {
        let mut input = rule_input();
        input
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        serde_json::from_value::<ReferenceInput>(input).unwrap_err();
    }
    for field in ["strength", "id", "revision"] {
        let mut input = rule_input();
        input["rules"]["entries"][4]
            .as_object_mut()
            .unwrap()
            .remove(field);
        serde_json::from_value::<ReferenceInput>(input).unwrap_err();
    }
}
