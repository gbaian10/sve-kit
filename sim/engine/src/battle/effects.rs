//! Frozen installed changes are distinct from effective card characteristics.

use super::{BattleReferences, ProjectionError, objects::ObjectScope, pending, sources};
use crate::catalog::Catalog;
#[cfg(all(test, feature = "runner"))]
use crate::game::player::ObjectHandle;
use crate::game::player::{Viewer, observation as native};
use crate::ir::{AbilityKind, CostKind, DamageKind};
use core::result;
use sve_battle_contract::{Int, Nat, Seat, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

/// Project installed changes on one currently authorized card.
///
/// Unknown current state stays absent; known empty effects stay an empty vector.
/// Runner fixtures supply observations directly; hosts use `Game::battle_observation`.
///
/// # Errors
/// Refuses mismatched catalogs, unavailable card handles or source coordinates,
/// links outside this observation and numbers outside the exact wire range.
#[cfg(all(test, feature = "runner"))]
pub(super) fn project_card_effects(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
    card: &ObjectHandle,
) -> Result<Option<Vec<wire::InstalledEffect>>> {
    references.validate_catalog(catalog)?;
    let objects = ObjectScope::new(observation)?;
    let member = observation
        .players
        .iter()
        .flat_map(|player| &player.zones)
        .flat_map(|zone| &zone.cards)
        .find(|member| &member.card.handle == card)
        .ok_or(ProjectionError::Unavailable)?;
    member
        .card
        .properties
        .state
        .as_ref()
        .map(|state| installed(catalog, references, &objects, &state.effects))
        .transpose()
}

/// Project the disclosed installed changes affecting one player.
///
/// Runner fixtures supply observations directly; hosts use `Game::battle_observation`.
///
/// # Errors
/// Refuses a mismatched catalog, invalid object scope, unavailable origins or
/// links, and numbers outside the exact wire range.
#[cfg(all(test, feature = "runner"))]
pub(super) fn project_player_effects(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
    player: Viewer,
) -> Result<Vec<wire::InstalledEffect>> {
    references.validate_catalog(catalog)?;
    let objects = ObjectScope::new(observation)?;
    let player = observation
        .players
        .iter()
        .find(|observed| observed.player == player)
        .ok_or(ProjectionError::Unavailable)?;
    installed(catalog, references, &objects, &player.effects)
}

pub(super) fn installed(
    catalog: &Catalog,
    references: &BattleReferences,
    objects: &ObjectScope<'_>,
    effects: &[native::InstalledEffect],
) -> Result<Vec<wire::InstalledEffect>> {
    effects
        .iter()
        .map(|effect| {
            Ok(wire::InstalledEffect {
                source: effect
                    .source
                    .as_ref()
                    .map(|origin| sources::source(catalog, references, objects, origin))
                    .transpose()?,
                expires: expiry(effect.expires),
                main_live: effect.main_live,
                intervals: effect.intervals.iter().copied().map(interval).collect(),
                active: effect.active,
                change: change(objects, &effect.change)?,
            })
        })
        .collect()
}

fn change(
    objects: &ObjectScope<'_>,
    change: &native::InstalledChange,
) -> Result<wire::InstalledChange> {
    Ok(match change {
        native::InstalledChange::Numeric { attack, life } => wire::InstalledChange::Numeric {
            attack: attack.map(number).transpose()?,
            life: life.map(number).transpose()?,
        },
        native::InstalledChange::Cost { kind, change } => wire::InstalledChange::Cost {
            cost: match kind {
                CostKind::Play => wire::CostKind::Play,
                CostKind::Evolution => wire::CostKind::Evolution,
            },
            change: number(*change)?,
        },
        native::InstalledChange::Grant {
            keywords,
            traits,
            definitions,
        } => wire::InstalledChange::Grant {
            keywords: keywords.clone(),
            traits: traits.clone(),
            definitions: definitions
                .iter()
                .map(|definition| {
                    Ok(wire::GrantedDefinition {
                        ability: ability_kind(definition.kind),
                        line: natural(definition.line)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        },
        native::InstalledChange::AbilityRemoval { boxed } => {
            wire::InstalledChange::AbilityRemoval { boxed: *boxed }
        }
        native::InstalledChange::Pilot => wire::InstalledChange::Pilot,
        native::InstalledChange::NoStand { player } => wire::InstalledChange::NoStand {
            player: seat(*player),
        },
        native::InstalledChange::AttackRestriction { leaders } => {
            wire::InstalledChange::AttackRestriction {
                leaders: leaders
                    .as_ref()
                    .map(|leaders| leaders.iter().copied().map(seat).collect()),
            }
        }
        native::InstalledChange::AbilityRestriction { except_evolution } => {
            wire::InstalledChange::AbilityRestriction {
                except_evolution: *except_evolution,
            }
        }
        native::InstalledChange::NoLeaderAttack => wire::InstalledChange::NoLeaderAttack,
        native::InstalledChange::Prevention {
            kind,
            change,
            remaining,
            recipients,
            sources,
        } => wire::InstalledChange::Prevention {
            damage: match kind {
                DamageKind::Any => wire::DamageKind::Any,
                DamageKind::Attack => return Err(ProjectionError::Unavailable),
                DamageKind::Combat => wire::DamageKind::Combat,
                DamageKind::Ability => wire::DamageKind::Ability,
            },
            change: prevention(*change)?,
            remaining: remaining.map(natural).transpose()?,
            recipients: recipients
                .as_ref()
                .map(|recipients| {
                    recipients
                        .iter()
                        .map(|recipient| effect_recipient(objects, recipient))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?,
            sources: sources
                .as_ref()
                .map(|sources| {
                    sources
                        .iter()
                        .map(|source| objects.handle(source))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?,
        },
        native::InstalledChange::PlayerCost { change, limited } => {
            wire::InstalledChange::PlayerCost {
                change: number(*change)?,
                limited: *limited,
            }
        }
        native::InstalledChange::BanishPlay => wire::InstalledChange::BanishPlay,
        native::InstalledChange::Delayed { timing } => wire::InstalledChange::Delayed {
            timing: pending::timing(*timing),
        },
    })
}

fn effect_recipient(
    objects: &ObjectScope<'_>,
    recipient: &native::EffectRecipient,
) -> Result<wire::EffectRecipient> {
    Ok(match recipient {
        native::EffectRecipient::Leader(player) => wire::EffectRecipient::Leader {
            player: seat(*player),
        },
        native::EffectRecipient::Card(card) => wire::EffectRecipient::Card {
            object: objects.handle(card)?,
        },
    })
}

fn number(value: native::NumberChange) -> Result<wire::NumberChange> {
    Ok(match value {
        native::NumberChange::Add(amount) => wire::NumberChange::Add {
            amount: Int::try_from(amount)?,
        },
        native::NumberChange::Set(amount) => wire::NumberChange::Set {
            amount: Int::try_from(amount)?,
        },
    })
}

fn prevention(value: native::PreventionChange) -> Result<wire::PreventionChange> {
    Ok(match value {
        native::PreventionChange::Prevent => wire::PreventionChange::Prevent,
        native::PreventionChange::Add(amount) => wire::PreventionChange::Add {
            amount: Int::try_from(amount)?,
        },
        native::PreventionChange::Cap {
            threshold,
            replacement,
        } => wire::PreventionChange::Cap {
            threshold: natural(threshold)?,
            replacement: natural(replacement)?,
        },
    })
}

fn natural(value: u64) -> Result<Nat> {
    Nat::try_from(value).map_err(ProjectionError::from)
}

pub(super) const fn ability_kind(value: AbilityKind) -> wire::AbilityKind {
    match value {
        AbilityKind::Spell => wire::AbilityKind::Spell,
        AbilityKind::Activated => wire::AbilityKind::Activated,
        AbilityKind::Triggered => wire::AbilityKind::Triggered,
        AbilityKind::Static => wire::AbilityKind::Static,
    }
}

const fn seat(value: Viewer) -> Seat {
    match value {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

const fn expiry(value: native::EffectExpiry) -> wire::EffectExpiry {
    match value {
        native::EffectExpiry::NoTemporalExpiry => wire::EffectExpiry::NoTemporalExpiry,
        native::EffectExpiry::ResolutionEnd => wire::EffectExpiry::ResolutionEnd,
        native::EffectExpiry::TurnEnd => wire::EffectExpiry::TurnEnd,
        native::EffectExpiry::NextTurnEnd(player) => wire::EffectExpiry::NextTurnEnd {
            player: seat(player),
        },
        native::EffectExpiry::SourceLeavesField => wire::EffectExpiry::SourceLeavesField,
        native::EffectExpiry::NextStartEnd(player) => wire::EffectExpiry::NextStartEnd {
            player: seat(player),
        },
    }
}

const fn interval(value: native::EffectInterval) -> wire::EffectInterval {
    match value {
        native::EffectInterval::NextTurn(player) => wire::EffectInterval::NextTurn {
            player: seat(player),
        },
        native::EffectInterval::CurrentTurn => wire::EffectInterval::CurrentTurn,
    }
}
