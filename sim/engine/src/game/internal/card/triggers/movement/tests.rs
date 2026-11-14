use super::*;
use crate::ir::{
    AbilityId, AbilityQualification, AbilitySource, Binding, CardKind, FaceId, Nat, ObjectOperand,
    PlayerRef, ScopeId, TriggerSpec,
};

/// A printed field observer of own followers arriving in the cemetery from the field.
fn observer(not_same_name: bool) -> Ability {
    let event = Binding::new(1, ScopeId(1));
    let filter = SubjectFilter {
        own: Some(true),
        kind: Some(CardKind::Follower),
        not_same_name,
        ..SubjectFilter::default()
    };
    Ability {
        id: AbilityId(1),
        scope: ScopeId(1),
        source: AbilitySource {
            holder: ObjectOperand::SourceHolder {},
            origin: AbilityOrigin::Printed {
                face: FaceId("0".into()),
                line: Nat(1),
            },
            controller: PlayerRef::Controller {},
        },
        kind: AbilityKind::Triggered,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: Some(TriggerSpec {
            on: TriggerCondition::Event {
                pattern: EventPattern::LeaveField {
                    binding: event,
                    predicate: filter.predicate(SubjectFields::Cemetery(event)),
                },
            },
            active_zones: vec![ZoneKind::Field],
            view: EventSnapshotPolicy::BeforeExit,
            limit: None,
            aggregation: TriggerAggregation::PerFact {},
        }),
        body: None,
        declarations: vec![],
    }
}

#[test]
fn departure_headers_reject_the_history_only_holder_name_exclusion_at_load() {
    assert!(
        check_header(&observer(false)).unwrap(),
        "the same header without the exclusion is an admitted movement observer"
    );
    assert!(
        check_header(&observer(true)).is_err(),
        "observers cannot compare with the holder's names, so loading rejects the header"
    );
}
