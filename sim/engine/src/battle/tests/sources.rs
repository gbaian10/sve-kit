use super::{board_fixture, rule_fixtures};
use crate::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput, sources::source_ref};
use crate::game::player::observation::{
    CardFace, EffectSource, TextProvenance, WordingField, WordingSource,
};
use serde_json::json;
use sve_battle_contract::{CardRef, Nat, ObjectHandle, SourceRef, TextRef};

#[expect(
    clippy::unwrap_used,
    reason = "Malformed synthetic closures fail fixture creation."
)]
fn references() -> BattleReferences {
    let fixture = board_fixture::fixture().unwrap();
    let faces = [
        ("unit-follower", 0_u64, "printing-follower", 1_u32),
        ("unit-follower", 1, "printing-follower", 0),
        ("unit-spell", 0, "printing-spell", 0),
        ("unit-amulet", 0, "printing-amulet", 0),
    ]
    .into_iter()
    .map(
        |(number, native_face, printing, face)| PrintingFaceIdentity {
            card_no: number.into(),
            native_face,
            card: CardRef {
                printing_id: printing.into(),
                face: Nat::from(face),
            },
        },
    )
    .collect();
    let wording = serde_json::from_value(json!({
        "manifest_sha256":format!("sha256:{}","a".repeat(64)),
        "faces":[
            {"card_no":"unit-follower","native_face":0_u64,"face_id":"front","revisions":[
                {"revision_id":"old","fields":[{"kind":"effect"},{"kind":"section","ordinal":0_u64},{"kind":"section","ordinal":2_u64}]}
            ]},
            {"card_no":"unit-follower","native_face":1_u64,"face_id":"back","revisions":[]},
            {"card_no":"unit-spell","native_face":0_u64,"face_id":"spell","revisions":[]},
            {"card_no":"unit-amulet","native_face":0_u64,"face_id":"amulet","revisions":[]}
        ]
    })).unwrap();
    BattleReferences::new(
        &fixture.catalog,
        ReferenceInput {
            rules: None,
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(fixture.snapshot.as_bytes()),
            faces,
            wording: Some(wording),
        },
    )
    .unwrap()
}

fn origin() -> EffectSource {
    EffectSource {
        card: "unit-follower".into(),
        face: Some(0),
        line: Some(3),
        holder: None,
        provenance: TextProvenance::Printed,
        wording: Some(Box::new(WordingSource {
            face_id: "front".into(),
            revision_id: "old".into(),
            field: WordingField::Effect,
        })),
    }
}

#[test]
fn exact_text_sources_preserve_owner_revision_and_canonical_sections() {
    let fixture = board_fixture::fixture().unwrap();
    let references = references();
    for (field, section) in [
        (WordingField::Effect, "effect"),
        (WordingField::Section { ordinal: 0 }, "section:0"),
        (WordingField::Section { ordinal: 2 }, "section:2"),
    ] {
        let mut source = origin();
        source.wording.as_mut().unwrap().field = field;
        assert_eq!(
            source_ref(&fixture.catalog, &references, &source, None),
            Ok(SourceRef::Text {
                text: TextRef {
                    card: CardRef {
                        printing_id: "printing-follower".into(),
                        face: Nat::from(1_u32)
                    },
                    wording_revision: "old".into(),
                    section: section.into(),
                    line: Nat::from(3_u32),
                    step: None,
                    granted_by: None,
                    holder: None
                }
            })
        );
    }
}

#[test]
fn exact_text_sources_use_the_direct_giver_even_for_same_printing_and_self_grants() {
    let fixture = board_fixture::fixture().unwrap();
    let references = references();
    for (number, native_face, printing, permanent_face) in [
        ("unit-follower", 0, "printing-follower", 1_u32),
        ("unit-follower", 1, "printing-follower", 0),
        ("unit-amulet", 0, "printing-amulet", 0),
    ] {
        let mut source = origin();
        source.provenance = TextProvenance::Granted {
            by: CardFace {
                card: number.into(),
                face: native_face,
            },
        };
        let holder = ObjectHandle::new("current-recipient".into());
        let SourceRef::Text { text } =
            source_ref(&fixture.catalog, &references, &source, Some(holder.clone())).unwrap()
        else {
            panic!("exact granted text")
        };
        assert_eq!(
            text.card,
            CardRef {
                printing_id: "printing-follower".into(),
                face: Nat::from(1_u32)
            }
        );
        assert_eq!(
            text.granted_by,
            Some(CardRef {
                printing_id: printing.into(),
                face: Nat::from(permanent_face)
            })
        );
        assert_eq!(text.holder, Some(holder));
        assert_eq!(text.step, None);
    }
}

#[test]
fn exact_text_sources_do_not_promote_unknown_or_unprovided_evidence() {
    let fixture = board_fixture::fixture().unwrap();
    let references = references();
    let expected = SourceRef::Card {
        printing_id: "printing-follower".into(),
        face: Some(Nat::from(1_u32)),
        line: Some(Nat::from(3_u32)),
        holder: None,
    };
    let mut source = origin();
    source.provenance = TextProvenance::Unknown;
    assert_eq!(
        source_ref(&fixture.catalog, &references, &source, None),
        Ok(expected.clone())
    );
    source.provenance = TextProvenance::Printed;
    assert_eq!(
        source_ref(&fixture.catalog, &fixture.references, &source, None),
        Ok(expected.clone())
    );
    source.wording = None;
    assert_eq!(
        source_ref(&fixture.catalog, &references, &source, None),
        Ok(expected)
    );
}

#[test]
fn exact_text_sources_reject_mismatched_owners_fields_coordinates_and_givers() {
    use crate::battle::ProjectionError;
    use sve_battle_contract::MAX_SAFE_INTEGER;
    let fixture = board_fixture::fixture().unwrap();
    let references = references();
    let mut cases = Vec::new();
    let mut wrong_owner = origin();
    wrong_owner.wording.as_mut().unwrap().face_id = "back".into();
    cases.push((wrong_owner, ProjectionError::Unavailable));
    let mut wrong_revision = origin();
    wrong_revision.wording.as_mut().unwrap().revision_id = "missing".into();
    cases.push((wrong_revision, ProjectionError::Unavailable));
    let mut absent_field = origin();
    absent_field.wording.as_mut().unwrap().field = WordingField::Section { ordinal: 1 };
    cases.push((absent_field, ProjectionError::Unavailable));
    let mut wrong_face = origin();
    wrong_face.face = Some(1);
    cases.push((wrong_face, ProjectionError::Unavailable));
    let mut absent_face = origin();
    absent_face.face = None;
    cases.push((absent_face, ProjectionError::Unavailable));
    for line in [None, Some(0), Some(MAX_SAFE_INTEGER + 1)] {
        let mut source = origin();
        source.line = line;
        cases.push((
            source,
            if line == Some(MAX_SAFE_INTEGER + 1) {
                ProjectionError::Range
            } else {
                ProjectionError::Unavailable
            },
        ));
    }
    let mut overflow_section = origin();
    overflow_section.wording.as_mut().unwrap().field = WordingField::Section {
        ordinal: MAX_SAFE_INTEGER + 1,
    };
    cases.push((overflow_section, ProjectionError::Range));
    let mut absent_giver = origin();
    absent_giver.provenance = TextProvenance::Granted {
        by: CardFace {
            card: "missing".into(),
            face: 0,
        },
    };
    cases.push((absent_giver, ProjectionError::Unavailable));
    for (source, error) in cases {
        assert_eq!(
            source_ref(&fixture.catalog, &references, &source, None),
            Err(error)
        );
    }
}
