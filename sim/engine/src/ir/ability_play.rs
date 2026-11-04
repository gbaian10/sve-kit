use super::{
    AbilityChoiceKind, AbilityCostPolicy, AbilityOperand, AbilityTriggerPolicy, Action, Binding,
    Cardinality, ChoiceDomain, Decision, DecisionKind, Declinable, Nat, ObjectOperand, PlayerRef,
    RuntimeAbilityReference, Visibility,
};

pub(crate) fn choice(
    decision: &Decision,
) -> Option<(
    &ObjectOperand,
    AbilityChoiceKind,
    Binding<RuntimeAbilityReference>,
)> {
    let ChoiceDomain::Abilities {
        holder,
        filter,
        binding,
    } = &decision.domain
    else {
        return None;
    };
    let single = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    (decision.captures.is_empty()
        && decision.actor == (PlayerRef::Controller {})
        && decision.visibility == (Visibility::Public {})
        && decision.kind
            == (DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: single,
                precondition: None,
            })
        && matches!(
            (filter, holder),
            (
                AbilityChoiceKind::EvolutionTime,
                ObjectOperand::SourceHolder {}
            ) | (
                AbilityChoiceKind::UnionBurst,
                ObjectOperand::Selected { .. }
            )
        ))
    .then_some((holder, *filter, *binding))
}

pub(crate) const fn selection(
    action: &Action,
) -> Option<(Binding<RuntimeAbilityReference>, AbilityCostPolicy)> {
    let Action::PlayAbility {
        ability: AbilityOperand::Selected { selection },
        controller: PlayerRef::Controller {},
        cost_policy,
        trigger_policy: AbilityTriggerPolicy::IgnoreAsAuthorized,
    } = action
    else {
        return None;
    };
    Some((*selection, *cost_policy))
}
