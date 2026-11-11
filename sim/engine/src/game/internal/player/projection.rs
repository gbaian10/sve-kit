//! Cached authority choices supply semantics; projection never advances their procedures.

use super::super::{Game, InputDomain, OrderCard, PlayerId, Result, Wait, invalid};
use crate::game::player::observation::ObservedZone;
use crate::game::player::{
    CardOption, DecisionShape, DiscardKind, MovementCapacityQuestion, PartitionQuestion,
    SearchQuestion, TokenCapacityQuestion, Viewer,
};
use crate::ir::{Cardinality, Nat, ZoneKind};

macro_rules! domain {
    ($domain:expr, $pattern:pat => $shape:expr) => {
        match $domain {
            $pattern => Ok($shape),
            _ => Err(invalid("inconsistent player decision shape")),
        }
    };
}

fn cards(choices: Vec<OrderCard>) -> Vec<CardOption> {
    choices
        .into_iter()
        .map(|choice| CardOption {
            handle: choice.handle,
            card: choice.card,
            name: choice.name,
        })
        .collect()
}

fn partition(domain: super::super::SearchPartitionDomain) -> PartitionQuestion {
    PartitionQuestion {
        cards: cards(domain.cards),
        cardinality: domain.cardinality,
    }
}

fn viewer(player: PlayerId) -> Result<Viewer> {
    match player.seat {
        1 => Ok(Viewer::P1),
        2 => Ok(Viewer::P2),
        _ => Err(invalid("invalid decision actor")),
    }
}

fn capacity(details: (PlayerId, ZoneKind, usize)) -> Result<(Viewer, ObservedZone, Nat)> {
    let zone = match details.1 {
        ZoneKind::Field => ObservedZone::Field,
        ZoneKind::Ex => ObservedZone::Ex,
        _ => return Err(invalid("unsupported capacity destination")),
    };
    Ok((
        viewer(details.0)?,
        zone,
        Nat(u64::try_from(details.2).map_err(invalid)?),
    ))
}

fn routes(affordable: &[usize]) -> Result<Vec<Nat>> {
    if affordable.is_empty() {
        return Err(invalid("empty exchange route question"));
    }
    affordable
        .iter()
        .map(|route| u64::try_from(*route).map(Nat).map_err(invalid))
        .collect()
}

fn collection(domain: InputDomain, wait: &Wait) -> Result<DecisionShape> {
    if matches!(wait, Wait::Order(_)) {
        return domain!(domain, InputDomain::Order(choices) => DecisionShape::Order(cards(choices)));
    }
    if matches!(wait, Wait::Drive(_)) {
        return domain!(domain, InputDomain::DriveTrigger { icon, targets } => DecisionShape::DriveTrigger { icon, targets: cards(targets) });
    }
    if matches!(wait, Wait::Search(_)) {
        return domain!(domain, InputDomain::Search(search) => DecisionShape::Search(SearchQuestion { cards: cards(search.cards), cardinality: search.cardinality, groups: search.groups }));
    }
    if matches!(wait, Wait::SearchPartition(_)) {
        return domain!(domain, InputDomain::SearchPartition(search) => DecisionShape::SearchPartition(partition(search)));
    }
    Err(invalid("inconsistent collection decision"))
}

fn typed(domain: InputDomain, wait: &Wait) -> Result<DecisionShape> {
    match wait {
        Wait::Dice(_) => {
            domain!(domain, InputDomain::DiceReroll { value, remaining } => DecisionShape::DiceReroll { value, remaining })
        }
        Wait::Ability(_) => {
            domain!(domain, InputDomain::Abilities(options) => DecisionShape::Abilities(options))
        }
        Wait::ResourceEntryPayment(_) => {
            domain!(domain, InputDomain::ResourceEntryPayment { materials, ep_allowed } => DecisionShape::ResourceEntryPayment { materials, ep_allowed })
        }
        Wait::WardEntry(_) => {
            domain!(domain, InputDomain::WardEntry { card, name, zone, position } => DecisionShape::WardEntry { card, name, zone, position })
        }
        Wait::Discard(_) => {
            domain!(domain, InputDomain::Targets(cards) => DecisionShape::Discard { kind: DiscardKind::Effect, cards })
        }
        Wait::CostDiscard(_) => {
            domain!(domain, InputDomain::CostMaterials(cards) => DecisionShape::Discard { kind: DiscardKind::Cost, cards })
        }
        Wait::OptionalCost => {
            domain!(domain, InputDomain::OptionalCost => DecisionShape::OptionalCost)
        }
        Wait::ResolutionPayment { .. } => {
            domain!(domain, InputDomain::OptionalCost => DecisionShape::ResolutionPayment)
        }
        Wait::CostMaterials(_) | Wait::MaterialParameter(_) => {
            domain!(domain, InputDomain::CostMaterials(cards) => DecisionShape::CostMaterials(cards))
        }
        Wait::ExchangeRoute { affordable } => {
            domain!(domain, InputDomain::Integer(_) => DecisionShape::ExchangeRoutes(routes(affordable)?))
        }
        Wait::TriggerQuota { .. } => {
            domain!(domain, InputDomain::TriggerQuota(quota) => DecisionShape::TriggerQuota(quota))
        }
        Wait::Targets(_) | Wait::ResolutionTargets(_) | Wait::RecipientTargets(_) => {
            domain!(domain, InputDomain::Targets(targets) => DecisionShape::Targets(targets))
        }
        Wait::Modes(_) => {
            domain!(domain, InputDomain::Modes { modes, cardinality, count } => DecisionShape::Modes { modes, cardinality, count })
        }
        Wait::Alternative(_) => {
            domain!(domain, InputDomain::Modes { modes, cardinality, count } => DecisionShape::ResolutionAlternative { modes, cardinality, count })
        }
        Wait::May { .. } => {
            domain!(domain, InputDomain::Integer(Cardinality { minimum: Nat(0), maximum: Some(Nat(1)) }) => DecisionShape::May)
        }
        Wait::TokenCapacity(wait) => {
            let (destination, zone, available) = capacity(wait.player_details())?;
            domain!(domain, InputDomain::TokenCapacity { choices } => DecisionShape::TokenCapacity(TokenCapacityQuestion { destination, zone, available, choices }))
        }
        Wait::MovementCapacity(wait) => {
            let (destination, zone, available) = capacity(wait.player_details())?;
            domain!(domain, InputDomain::SearchPartition(selection) => DecisionShape::MovementCapacity(MovementCapacityQuestion { destination, zone, available, selection: partition(selection) }))
        }
        Wait::Distribution(_) => {
            domain!(domain, InputDomain::Distribution(distribution) => DecisionShape::Distribution(distribution))
        }
        Wait::Order(_) | Wait::Drive(_) | Wait::Search(_) | Wait::SearchPartition(_) => {
            collection(domain, wait)
        }
        Wait::DamageReplacements(_) => {
            domain!(domain, InputDomain::DamageReplacements(options) => DecisionShape::DamageReplacements(options))
        }
        Wait::SearchReplacement(_) => {
            domain!(domain, InputDomain::SearchReplacement => DecisionShape::SearchReplacement)
        }
        Wait::Integer { .. } => {
            domain!(domain, InputDomain::Integer(bounds) => DecisionShape::Integer(bounds))
        }
        Wait::Draw { .. } => domain!(domain, InputDomain::BeforeDraw => DecisionShape::BeforeDraw),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player shapes read the existing authority's cached questions."
)]
impl Game {
    pub(in crate::game) fn player_decision_shape(&self) -> Result<DecisionShape> {
        if let Some((view, wait)) = self
            .typed
            .as_ref()
            .and_then(|execution| execution.waiting.as_ref())
        {
            viewer(view.actor)?;
            return typed(view.domain.clone(), wait);
        }
        if let Some((view, _)) = &self.card_runtime.question {
            viewer(view.actor)?;
            return match &view.domain {
                InputDomain::Pending(options) => Ok(DecisionShape::Pending(options.clone())),
                InputDomain::TriggerQuota(quota) => Ok(DecisionShape::TriggerQuota(quota.clone())),
                _ => Err(invalid("inconsistent pending decision shape")),
            };
        }
        if let Some(question) = &self.card_runtime.quick {
            viewer(question.view.actor)?;
            return domain!(question.view.domain.clone(), InputDomain::Quick { window, options } => DecisionShape::Quick { window, options });
        }
        Err(invalid("missing player decision"))
    }

    pub(in crate::game) fn validate_player_decision_projection(&self) -> Result<()> {
        if self.state.game["ended"] != true && self.player_decision().is_some() {
            self.player_decision_shape()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
