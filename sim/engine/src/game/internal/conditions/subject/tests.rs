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
