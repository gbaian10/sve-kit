//! Cost quotes preserve uncertainty and never stand in for payment legality.

use super::ProjectionError;
use crate::game::player::{self, Viewer, observation as native};
use alloc::collections::BTreeSet;
use core::result;
use sve_battle_contract::{Int, Nat, ObjectHandle, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

/// Project the costs in one authority-produced, viewer-authorized observation.
///
/// Hosts keep this input bound to its authenticated viewer and stable stop.
/// The complete board projection must use the same current object handles.
///
/// # Errors
/// Refuses unknown, duplicate or cross-zone quote subjects, inconsistent object
/// scopes, and numbers outside the wire's exact integer range.
pub fn project_costs(observation: &native::Observation) -> Result<wire::CostObservation> {
    let objects = CostObjects::new(observation)?;
    let mut quoted = BTreeSet::new();
    let cards = observation
        .costs
        .cards
        .iter()
        .map(|quote| {
            if !objects.quotable.contains(&quote.card)
                || !quoted.insert(&quote.card)
                || quote.cost.basis != native::QuoteBasis::CurrentContext
            {
                return Err(ProjectionError::Unavailable);
            }
            Ok(wire::CardCostQuote {
                object: objects.handle(&quote.card)?,
                cost: summary(&quote.cost)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let current_play = observation
        .costs
        .current_play
        .as_ref()
        .map(|play| -> Result<wire::PlayCostQuote> {
            if play.cost.basis != native::QuoteBasis::BeforePayment {
                return Err(ProjectionError::Unavailable);
            }
            Ok(wire::PlayCostQuote {
                source: play
                    .source
                    .as_ref()
                    .map(|source| objects.handle(source))
                    .transpose()?,
                cost: summary(&play.cost)?,
            })
        })
        .transpose()?;
    Ok(wire::CostObservation {
        cards,
        current_play,
    })
}

struct CostObjects<'observation> {
    known: BTreeSet<&'observation player::ObjectHandle>,
    quotable: BTreeSet<&'observation player::ObjectHandle>,
}

impl<'observation> CostObjects<'observation> {
    fn new(observation: &'observation native::Observation) -> Result<Self> {
        if observation
            .players
            .iter()
            .map(|player| player.player)
            .ne([Viewer::P1, Viewer::P2])
        {
            return Err(ProjectionError::Unavailable);
        }
        let mut objects = Self {
            known: BTreeSet::new(),
            quotable: BTreeSet::new(),
        };
        for player in &observation.players {
            for zone in &player.zones {
                for member in &zone.cards {
                    let handle = &member.card.handle;
                    if !objects.known.insert(handle) {
                        return Err(ProjectionError::Unavailable);
                    }
                    if player.player == observation.viewer
                        && member.card.properties.controller == observation.viewer
                        && matches!(
                            zone.kind,
                            native::ObservedZone::Hand | native::ObservedZone::Ex
                        )
                    {
                        objects.quotable.insert(handle);
                    }
                }
            }
        }
        Ok(objects)
    }

    fn handle(&self, handle: &player::ObjectHandle) -> Result<ObjectHandle> {
        if !self.known.contains(handle) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(ObjectHandle::new(handle.as_str().into()))
    }
}

fn summary(cost: &native::CostSummary) -> Result<wire::CostSummary> {
    Ok(wire::CostSummary {
        basis: match cost.basis {
            native::QuoteBasis::CurrentContext => wire::QuoteBasis::CurrentContext,
            native::QuoteBasis::BeforePayment => wire::QuoteBasis::BeforePayment,
        },
        total_pp: cost.total_pp.map(integer).transpose()?,
        groups: cost.groups.iter().map(group).collect::<Result<Vec<_>>>()?,
        pending: cost
            .pending
            .iter()
            .map(|pending| match pending {
                native::PendingCost::X => wire::PendingCost::X,
                native::PendingCost::Optional => wire::PendingCost::Optional,
                native::PendingCost::Modes => wire::PendingCost::Modes,
                native::PendingCost::Materials => wire::PendingCost::Materials,
                native::PendingCost::Exchange => wire::PendingCost::Exchange,
                native::PendingCost::EntryResources => wire::PendingCost::EntryResources,
                native::PendingCost::Unavailable => wire::PendingCost::Unavailable,
            })
            .collect(),
    })
}

fn group(cost: &native::CostGroup) -> Result<wire::CostGroup> {
    Ok(wire::CostGroup {
        role: match cost.kind {
            native::CostGroupKind::Original => wire::CostGroupRole::Original,
            native::CostGroupKind::Additional => wire::CostGroupRole::Additional,
            native::CostGroupKind::Mode => wire::CostGroupRole::Mode,
            native::CostGroupKind::Optional { required, accepted } => {
                wire::CostGroupRole::Optional { required, accepted }
            }
        },
        lines: cost.lines.iter().map(line).collect::<Result<Vec<_>>>()?,
    })
}

fn line(cost: &native::CostLine) -> Result<wire::CostLine> {
    Ok(match cost {
        native::CostLine::Pp(quote) => wire::CostLine::Pp { quote: pp(*quote)? },
        native::CostLine::Posture { cards } => wire::CostLine::Posture {
            cards: cards.map(natural).transpose()?,
        },
        native::CostLine::Life { change } => wire::CostLine::Life {
            change: change.map(integer).transpose()?,
        },
        native::CostLine::Counter { name, count } => wire::CostLine::Counter {
            name: name.clone(),
            count: count.map(natural).transpose()?,
        },
        native::CostLine::Move { cards } => wire::CostLine::Move {
            cards: cards.map(natural).transpose()?,
        },
        native::CostLine::Discard { cards, selected } => wire::CostLine::Discard {
            cards: cards.map(natural).transpose()?,
            selected: *selected,
        },
        native::CostLine::Banish { cards } => wire::CostLine::Banish {
            cards: cards.map(natural).transpose()?,
        },
        native::CostLine::Reveal { cards } => wire::CostLine::Reveal {
            cards: cards.map(natural).transpose()?,
        },
        native::CostLine::Fusion { cards, selected } => wire::CostLine::Fusion {
            cards: cards.map(natural).transpose()?,
            selected: *selected,
        },
        native::CostLine::EntryResources { cards, use_ep } => wire::CostLine::EntryResources {
            cards: natural(*cards)?,
            use_ep: *use_ep,
        },
        native::CostLine::Other => wire::CostLine::Other,
    })
}

fn pp(quote: native::PpQuote) -> Result<wire::PpQuote> {
    Ok(match quote {
        native::PpQuote::Known(amount) => wire::PpQuote::Known {
            amount: integer(amount)?,
        },
        native::PpQuote::Variable { fixed, copies, x } => wire::PpQuote::Variable {
            fixed: integer(fixed)?,
            copies: natural(copies)?,
            x: x.map(natural).transpose()?,
        },
        native::PpQuote::Unavailable => wire::PpQuote::Unavailable,
    })
}

fn natural(value: u64) -> Result<Nat> {
    Nat::try_from(value).map_err(ProjectionError::from)
}

fn integer(value: i64) -> Result<Int> {
    Int::try_from(value).map_err(ProjectionError::from)
}
