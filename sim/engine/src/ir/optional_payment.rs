//! One optional payment declared before targets and committed before play.
use super::{
    Ability, Control, CostSpec, PaymentOptionality, PaymentTime, PlaySpec, Program, ReceiptId,
};

pub(crate) fn declaration(body: &Program) -> Option<(&CostSpec, ReceiptId)> {
    let super::Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs,
                optional: PaymentOptionality::Optional,
                on_paid,
                receipt,
            },
    } = &body.steps.first()?.node
    else {
        return None;
    };
    on_paid.steps.is_empty().then_some((costs, *receipt))
}

pub(crate) fn trigger_play(ability: &Ability) -> bool {
    ability.play.as_ref().is_none_or(|play| {
        matches!(play, PlaySpec { limit: None, captures, costs: CostSpec::Ordered { steps }, .. }
            if captures.is_empty() && steps.is_empty())
            && ability
                .body
                .as_ref()
                .is_some_and(|body| declaration(body).is_some())
    })
}
