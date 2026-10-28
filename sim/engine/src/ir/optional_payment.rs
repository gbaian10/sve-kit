//! One optional payment declared before targets and committed before play.
use super::{
    Ability, AbilityOrigin, Control, CostSpec, KeywordId, PaymentOptionality, PaymentTime,
    PlaySpec, Program, ReceiptId,
};

pub(crate) fn declaration(body: &Program) -> Option<(&CostSpec, ReceiptId)> {
    let super::Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs,
                on_paid,
                receipt,
                ..
            },
    } = &body.steps.first()?.node
    else {
        return None;
    };
    on_paid.steps.is_empty().then_some((costs, *receipt))
}

pub(crate) fn required(body: &Program) -> bool {
    matches!(
        body.steps.first().map(|step| &step.node),
        Some(super::Node::Control {
            control: Control::Pay {
                optional: PaymentOptionality::Required,
                ..
            }
        })
    )
}

pub(crate) fn trigger_play(ability: &Ability) -> bool {
    ability.play.as_ref().is_none_or(|_| {
        matches!(
            ability.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Fanfare,
                ..
            }
        ) || (ability.body.as_ref().is_some_and(required)
            && matches!(
                ability.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnEvolve
                        | KeywordId::OnSuperEvolve
                        | KeywordId::OnAttack
                        | KeywordId::OnRace
                        | KeywordId::OnDriveGain,
                    ..
                }
            ))
    }) && ability.play.as_ref().is_none_or(|play| {
        matches!(play, PlaySpec { limit: None, captures, costs: CostSpec::Ordered { steps }, .. }
            if captures.is_empty() && steps.is_empty())
            && ability
                .body
                .as_ref()
                .is_some_and(|body| declaration(body).is_some())
    })
}
