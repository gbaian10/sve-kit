//! Disclosed occurrences are observations, never answerable authority identities.

use super::{BattleReferences, ProjectionError, objects::ObjectScope, sources};
use crate::catalog::Catalog;
use crate::game::player::{Viewer, observation as native};
use core::result;
use sve_battle_contract::{Nat, Seat, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

/// Project disclosed pending occurrences from one authority-produced observation.
///
/// Runner fixtures supply observations directly; hosts use `Game::battle_observation`
/// so all board components share one authority read and object scope.
///
/// # Errors
/// Refuses mismatched catalogs, unknown printing or face coordinates, foreign
/// current handles, cross-viewer admission budgets and inexact wire integers.
#[cfg(all(test, feature = "runner"))]
pub(super) fn project_pending(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
) -> Result<Vec<wire::PendingTrigger>> {
    references.validate_catalog(catalog)?;
    let objects = ObjectScope::new(observation)?;
    scoped(catalog, references, observation, &objects)
}

pub(super) fn scoped(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
    objects: &ObjectScope<'_>,
) -> Result<Vec<wire::PendingTrigger>> {
    observation
        .pending
        .iter()
        .map(|pending| {
            if pending.controller != observation.viewer && pending.admission_remaining.is_some() {
                return Err(ProjectionError::Unavailable);
            }
            Ok(wire::PendingTrigger {
                source: sources::source(catalog, references, objects, &pending.source)?,
                controller: match pending.controller {
                    Viewer::P1 => Seat::P1,
                    Viewer::P2 => Seat::P2,
                },
                timing: timing(pending.timing),
                admission_remaining: pending
                    .admission_remaining
                    .map(Nat::try_from)
                    .transpose()
                    .map_err(ProjectionError::from)?,
            })
        })
        .collect()
}

pub(super) const fn timing(value: native::TriggerTiming) -> wire::TriggerTiming {
    match value {
        native::TriggerTiming::EnterField => wire::TriggerTiming::EnterField,
        native::TriggerTiming::Targeted => wire::TriggerTiming::Targeted,
        native::TriggerTiming::Resolve => wire::TriggerTiming::Resolve,
        native::TriggerTiming::Payment => wire::TriggerTiming::Payment,
        native::TriggerTiming::CounterChange => wire::TriggerTiming::CounterChange,
        native::TriggerTiming::EnterEx => wire::TriggerTiming::EnterEx,
        native::TriggerTiming::LeaveField => wire::TriggerTiming::LeaveField,
        native::TriggerTiming::Attack => wire::TriggerTiming::Attack,
        native::TriggerTiming::Evolve => wire::TriggerTiming::Evolve,
        native::TriggerTiming::Phase => wire::TriggerTiming::Phase,
        native::TriggerTiming::StatChange => wire::TriggerTiming::StatChange,
        native::TriggerTiming::Posture => wire::TriggerTiming::Posture,
        native::TriggerTiming::Destroy => wire::TriggerTiming::Destroy,
        native::TriggerTiming::UnionBurst => wire::TriggerTiming::UnionBurst,
        native::TriggerTiming::Fusion => wire::TriggerTiming::Fusion,
        native::TriggerTiming::Play => wire::TriggerTiming::Play,
        native::TriggerTiming::Discard => wire::TriggerTiming::Discard,
        native::TriggerTiming::HandReveal => wire::TriggerTiming::HandReveal,
        native::TriggerTiming::Banish => wire::TriggerTiming::Banish,
        native::TriggerTiming::Draw => wire::TriggerTiming::Draw,
        native::TriggerTiming::DriveGained => wire::TriggerTiming::DriveGained,
        native::TriggerTiming::DriveChecked => wire::TriggerTiming::DriveChecked,
        native::TriggerTiming::DriveTrigger => wire::TriggerTiming::DriveTrigger,
        native::TriggerTiming::Dice => wire::TriggerTiming::Dice,
        native::TriggerTiming::Equip => wire::TriggerTiming::Equip,
        native::TriggerTiming::LimitBreak => wire::TriggerTiming::LimitBreak,
        native::TriggerTiming::FinalLimitBreak => wire::TriggerTiming::FinalLimitBreak,
        native::TriggerTiming::Move => wire::TriggerTiming::Move,
        native::TriggerTiming::Race => wire::TriggerTiming::Race,
        native::TriggerTiming::LifeChange => wire::TriggerTiming::LifeChange,
        native::TriggerTiming::Damage => wire::TriggerTiming::Damage,
    }
}
