//! Cached authority choices supply semantics; projection never advances their procedures.

use super::super::{Execution, Game, InputDomain, PlayerId, Result, Wait, invalid};
use crate::game::player::observation::ObservedZone;
use crate::game::player::{
    DecisionShape, DiscardKind, MovementCapacityQuestion, SelectionQuestion, TokenCapacityQuestion,
    Viewer,
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

fn collection(
    game: &Game,
    execution: &Execution,
    actor: Viewer,
    domain: InputDomain,
    wait: &Wait,
) -> Result<DecisionShape> {
    match (wait, domain) {
        (Wait::Order(wait), InputDomain::Order(cards)) => Ok(DecisionShape::Order(
            wait.player_cards(game, actor, &cards)?,
        )),
        (Wait::Drive(wait), InputDomain::DriveTrigger { icon, targets }) => {
            Ok(DecisionShape::DriveTrigger {
                icon,
                targets: wait.player_cards(game, actor, &targets)?,
            })
        }
        (Wait::Search(wait), InputDomain::Search(domain)) => Ok(DecisionShape::Search(
            wait.player_question(game, execution, actor, &domain)?,
        )),
        (Wait::SearchPartition(wait), InputDomain::SearchPartition(domain)) => Ok(
            DecisionShape::SearchPartition(wait.player_question(game, actor, &domain)?),
        ),
        _ => Err(invalid("inconsistent collection decision")),
    }
}

fn selection(
    game: &Game,
    actor: Viewer,
    domain: InputDomain,
    wait: &Wait,
) -> Result<DecisionShape> {
    match (wait, domain) {
        (
            Wait::ResourceEntryPayment(wait),
            InputDomain::ResourceEntryPayment {
                materials,
                ep_allowed,
            },
        ) => Ok(DecisionShape::ResourceEntryPayment {
            materials: wait.player_question(game, actor, &materials)?,
            ep_allowed,
        }),
        (Wait::Discard(wait), InputDomain::Targets(domain)) => Ok(DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards: wait.player_question(game, actor, &domain)?,
        }),
        (Wait::CostDiscard(wait), InputDomain::CostMaterials(domain)) => {
            Ok(DecisionShape::Discard {
                kind: DiscardKind::Cost,
                cards: wait.player_question(game, actor, &domain)?,
            })
        }
        (Wait::CostMaterials(wait), InputDomain::CostMaterials(domain)) => Ok(
            DecisionShape::CostMaterials(wait.player_question(game, actor, &domain)?),
        ),
        (Wait::MaterialParameter(wait), InputDomain::CostMaterials(domain)) => Ok(
            DecisionShape::CostMaterials(wait.player_question(game, actor, &domain)?),
        ),
        (Wait::Targets(wait) | Wait::ResolutionTargets(wait), InputDomain::Targets(domain)) => Ok(
            DecisionShape::Targets(wait.player_question(game, actor, &domain)?),
        ),
        (Wait::RecipientTargets(wait), InputDomain::Targets(domain)) => Ok(DecisionShape::Targets(
            wait.player_question(game, actor, &domain)?,
        )),
        _ => Err(invalid("inconsistent selection decision")),
    }
}

fn typed(
    game: &Game,
    execution: &Execution,
    actor: Viewer,
    domain: InputDomain,
    wait: &Wait,
) -> Result<DecisionShape> {
    match wait {
        Wait::Dice(_) => {
            domain!(domain, InputDomain::DiceReroll { value, remaining } => DecisionShape::DiceReroll { value, remaining })
        }
        Wait::Ability(wait) => {
            domain!(domain, InputDomain::Abilities(options) => DecisionShape::Abilities(wait.player_choices(game, actor, &options)?))
        }
        Wait::ResourceEntryPayment(_)
        | Wait::Discard(_)
        | Wait::CostDiscard(_)
        | Wait::CostMaterials(_)
        | Wait::MaterialParameter(_)
        | Wait::Targets(_)
        | Wait::ResolutionTargets(_)
        | Wait::RecipientTargets(_) => selection(game, actor, domain, wait),
        Wait::WardEntry { reference, links } => {
            domain!(domain, InputDomain::WardEntry { card, name, zone, position } => DecisionShape::WardEntry { card, name, zone, position, object: links.observed(game, actor, Some(*reference))? })
        }
        Wait::OptionalCost => {
            domain!(domain, InputDomain::OptionalCost => DecisionShape::OptionalCost)
        }
        Wait::ResolutionPayment { .. } => {
            domain!(domain, InputDomain::OptionalCost => DecisionShape::ResolutionPayment)
        }
        Wait::ExchangeRoute { affordable } => {
            domain!(domain, InputDomain::Integer(_) => DecisionShape::ExchangeRoutes(routes(affordable)?))
        }
        Wait::TriggerQuota { .. } => {
            domain!(domain, InputDomain::TriggerQuota(quota) => DecisionShape::TriggerQuota(game.player_quota_question(actor, &quota)?))
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
        // Same template-combination handles, addressed to the decider for the destination's EX.
        Wait::TransformTemplates(wait) => {
            let (destination, zone, available) = capacity(wait.player_details())?;
            domain!(domain, InputDomain::TokenCapacity { choices } => DecisionShape::TokenCapacity(TokenCapacityQuestion { destination, zone, available, choices }))
        }
        Wait::MovementCapacity(wait) => {
            let (destination, zone, available) = capacity(wait.player_details())?;
            domain!(domain, InputDomain::SearchPartition(selection) => DecisionShape::MovementCapacity(MovementCapacityQuestion { destination, zone, available, selection: wait.player_question(game, actor, &selection)? }))
        }
        Wait::Distribution(wait) => {
            domain!(domain, InputDomain::Distribution(distribution) => DecisionShape::Distribution(wait.player_question(game, actor, &distribution)?))
        }
        Wait::Order(_) | Wait::Drive(_) | Wait::Search(_) | Wait::SearchPartition(_) => {
            collection(game, execution, actor, domain, wait)
        }
        Wait::DamageReplacements(wait) => {
            domain!(domain, InputDomain::DamageReplacements(options) => DecisionShape::DamageReplacements(wait.player_options(game, actor, &options)?))
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
        if let Some(execution) = &self.typed
            && let Some((view, wait)) = &execution.waiting
        {
            return typed(
                self,
                execution,
                viewer(view.actor)?,
                view.domain.clone(),
                wait,
            );
        }
        if let Some((view, ids)) = &self.card_runtime.question {
            let actor = viewer(view.actor)?;
            return match &view.domain {
                InputDomain::Pending(options) => Ok(DecisionShape::Pending(
                    self.player_pending_options(actor, options, ids)?,
                )),
                InputDomain::TriggerQuota(quota) => Ok(DecisionShape::TriggerQuota(
                    self.player_quota_question(actor, quota)?,
                )),
                _ => Err(invalid("inconsistent pending decision shape")),
            };
        }
        if let Some(question) = &self.card_runtime.quick {
            let actor = viewer(question.view.actor)?;
            return domain!(question.view.domain.clone(), InputDomain::Quick { window, options } => DecisionShape::Quick { window, options: question.player_options(self, actor, &options)? });
        }
        Err(invalid("missing player decision"))
    }

    pub(in crate::game::internal) fn player_evolution_selection(
        &self,
        actor: Viewer,
        execution: &Execution,
    ) -> Result<SelectionQuestion> {
        let Some((view, wait)) = &execution.waiting else {
            return Err(invalid("missing evolution selection question"));
        };
        match (wait, &view.domain) {
            (Wait::MaterialParameter(wait), InputDomain::CostMaterials(domain)) => {
                wait.player_question(self, actor, domain)
            }
            (Wait::CostDiscard(wait), InputDomain::CostMaterials(domain)) => {
                wait.player_question(self, actor, domain)
            }
            _ => Err(invalid("inconsistent evolution selection question")),
        }
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
