use super::*;
use crate::ir::{CardName, ClassId};

#[test]
fn captured_projection_limits_survive_not_and_do_not_promote_saved_but_unauthorized_fields() {
    let information = PublicCharacteristics {
        kind: Some(CardKind::Follower),
        token: false,
        class: Some(ClassId("Neutral".into())),
        traits: vec![TraitId("Golem".into())],
        names: vec![CardName("saved name".into())],
    };
    let reader = Captured {
        information: Information::Source(&information),
        context: Context::Unavailable,
    };
    assert!(source(&information, CardKind::Follower, &information.traits[0]).unwrap());
    for condition in [
        boolean(BooleanProperty::SameSource, false),
        boolean(BooleanProperty::KnownClass, true),
        original_cost(Comparison::LessEqual, 0),
        labels(LabelsProperty::Names, vec!["saved name".into()]),
        labels(
            LabelsProperty::Class(CharacteristicView::Effective),
            vec!["Neutral".into()],
        ),
        boolean(BooleanProperty::State(BooleanState::TokenStatus), false),
    ] {
        assert!(matches!(
            evaluate(condition, &reader),
            Err(crate::EngineFailure::Unsupported(_))
        ));
    }
    let traits_only = Captured {
        information: Information::Traits(&information.traits),
        context: Context::Unavailable,
    };
    assert!(matches!(
        evaluate(
            Tree::Atom(Atom::Kind(
                CharacteristicView::Effective,
                CardKind::Follower
            )),
            &traits_only
        ),
        Err(crate::EngineFailure::Unsupported(_))
    ));
    let controller = PlayerId { seat: 1 };
    let controller_only = Captured {
        information: Information::None,
        context: Context::Controller(controller),
    };
    assert!(controlled_by(controller, controller).unwrap());
    assert!(matches!(
        evaluate(
            labels(
                LabelsProperty::Traits(CharacteristicView::Effective),
                vec!["Golem".into()]
            ),
            &controller_only
        ),
        Err(crate::EngineFailure::Unsupported(_))
    ));
}

#[test]
fn a_missing_printed_cost_is_never_zero_in_an_event_comparison() {
    let observer = PlayerId { seat: 1 };
    for (comparison, threshold) in [
        (Comparison::Equal, 0),
        (Comparison::LessEqual, 2),
        (Comparison::GreaterEqual, 0),
    ] {
        let filter = SubjectFilter {
            original_cost: Some((comparison, threshold)),
            ..SubjectFilter::default()
        };
        for cost in [None, Some(0)] {
            let information = SubjectInformation {
                original_cost: cost,
                keywords: &[],
                kind: Some(CardKind::Follower),
                traits: &[],
                names: &[],
                class: None,
            };
            assert_eq!(
                matches(
                    &filter,
                    &information,
                    false,
                    observer,
                    observer,
                    observer,
                    false
                )
                .unwrap(),
                cost.is_some()
            );
        }
    }
}

#[test]
fn not_class_needs_captured_class_information_before_negating() {
    let observer = PlayerId { seat: 1 };
    let forbidden = ClassId("Dragon".into());
    let other = ClassId("Neutral".into());
    let filter = SubjectFilter {
        not_class: Some(forbidden.clone()),
        ..SubjectFilter::default()
    };
    for (class, expected) in [
        (None, false),
        (Some(&forbidden), false),
        (Some(&other), true),
    ] {
        let information = SubjectInformation {
            original_cost: None,
            keywords: &[],
            kind: Some(CardKind::Follower),
            traits: &[],
            names: &[],
            class,
        };
        assert_eq!(
            matches(
                &filter,
                &information,
                false,
                observer,
                observer,
                observer,
                false
            )
            .unwrap(),
            expected
        );
    }
}
