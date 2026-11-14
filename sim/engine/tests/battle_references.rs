//! Printing identities are explicit, complete and bound to exact snapshot bytes.

use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_battle_contract::{CardRef, Nat};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ProjectionError, ReferenceInput};
use sve_engine::catalog::{Catalog, EngineIdentityInput, FaceIdentity};
use sve_engine::game::player::observation::CardFace;

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
        let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
        let documents = vec![("synthetic.yaml".into(), json!({"format":1_u8,"kind":"effect_set","cards":{
            "nativeⓈ":{"status":"complete","abilities":[]},
            "nativeⓈEN":{"status":"complete","abilities":[]}
        }}).to_string())];
        let rules = rule_fixtures::rules(
            &snapshot,
            json!([rule_fixtures::title("synthetic", "nativeⓈ", json!([]))]),
            json!([]),
        );
        let catalog = Catalog::from_documents_with_rules(
            &snapshot, &keywords, &documents, &rules.to_string(), &EngineIdentityInput::LegacyJp,
        ).unwrap();
        let input = ReferenceInput {
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
        Catalog::from_documents(&source.snapshot, &source.keywords, &source.documents).unwrap();
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
    let catalog = Catalog::from_documents_with_rules(
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
    let other = Catalog::from_documents_with_rules(
        &snapshot,
        &source.keywords,
        &source.documents,
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
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
