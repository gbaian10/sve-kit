use super::{Action, CostOverride, CostSpec, EffectReference, Int, ObjectOperand, PlayerRef};

pub(crate) fn card_operand(action: &Action) -> Option<&ObjectOperand> {
    let Action::PlayCard {
        card,
        controller,
        cost_override,
        authorization,
        parameters,
    } = action
    else {
        return None;
    };
    (matches!(card, ObjectOperand::Selected { .. })
        && *controller == (PlayerRef::Controller {})
        && matches!(
            cost_override,
            Some(CostOverride::SetBase { amount: Int(0) })
        )
        && *authorization == (EffectReference::CurrentInstallation {})
        && parameters.limit.is_none()
        && parameters.captures.is_empty()
        && parameters.parameters.is_empty()
        && matches!(&parameters.costs, CostSpec::Ordered { steps } if steps.is_empty()))
    .then_some(card)
}
