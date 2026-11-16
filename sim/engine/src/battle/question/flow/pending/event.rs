use crate::battle::ProjectionError;
use crate::game::player::PendingEvent;
use crate::ir::{EvolutionState, Phase, Posture, ZoneKind};
use sve_battle_contract::{Int, Nat, Zone, observation as wire, question::TriggerEvent};

pub(super) fn project(event: &PendingEvent) -> Result<TriggerEvent, ProjectionError> {
    Ok(match event {
        PendingEvent::DriveGained => TriggerEvent::DriveGained,
        PendingEvent::Race { ordinal } => TriggerEvent::Race {
            ordinal: Nat::try_from(*ordinal)?,
        },
        PendingEvent::EnterField { rules_names, from } => TriggerEvent::EnterField {
            rules_names: rules_names.clone(),
            from: from.map(zone),
        },
        PendingEvent::EnterEx => TriggerEvent::EnterEx,
        PendingEvent::StatChange => TriggerEvent::StatChange,
        PendingEvent::Posture { posture } => TriggerEvent::Posture {
            posture: match posture {
                Posture::Standing => wire::Posture::Standing,
                Posture::Acted => wire::Posture::Acted,
            },
        },
        PendingEvent::Destroy => TriggerEvent::Destroy,
        PendingEvent::LeaveField => TriggerEvent::LeaveField,
        PendingEvent::UnionBurst => TriggerEvent::UnionBurst,
        PendingEvent::Fusion => TriggerEvent::Fusion,
        PendingEvent::FusedAsMaterial => TriggerEvent::FusedAsMaterial,
        PendingEvent::Play => TriggerEvent::Play,
        PendingEvent::Discard => TriggerEvent::Discard,
        PendingEvent::Banish => TriggerEvent::Banish,
        PendingEvent::Draw => TriggerEvent::Draw,
        PendingEvent::DriveChecked => TriggerEvent::DriveChecked,
        PendingEvent::DriveTrigger => TriggerEvent::DriveTrigger,
        PendingEvent::Dice { value } => TriggerEvent::Dice {
            value: Nat::try_from(*value)?,
        },
        PendingEvent::Equip => TriggerEvent::Equip,
        PendingEvent::Move { player, from, to } => TriggerEvent::Move {
            player: super::common::seat(*player),
            from: zone(*from),
            to: zone(*to),
        },
        PendingEvent::Attack { ordinal } => TriggerEvent::Attack {
            ordinal: Nat::try_from(*ordinal)?,
        },
        PendingEvent::Phase {
            phase,
            turn,
            player,
        } => TriggerEvent::Phase {
            phase: match phase {
                Phase::Start => wire::Phase::Start,
                Phase::Main => wire::Phase::Main,
                Phase::End => wire::Phase::End,
            },
            turn: Nat::try_from(*turn)?,
            player: super::common::seat(*player),
        },
        PendingEvent::Evolve { state } => TriggerEvent::Evolve {
            state: match state {
                EvolutionState::Base => wire::EvolutionState::Base,
                EvolutionState::Evolved => wire::EvolutionState::Evolved,
                EvolutionState::SuperEvolved => wire::EvolutionState::SuperEvolved,
            },
        },
        PendingEvent::LifeChange { delta, player } => TriggerEvent::LifeChange {
            delta: Int::try_from(*delta)?,
            player: super::common::seat(*player),
        },
        PendingEvent::Damage { amount } => TriggerEvent::Damage {
            amount: Nat::try_from(*amount)?,
        },
    })
}

const fn zone(zone: ZoneKind) -> Zone {
    match zone {
        ZoneKind::Deck => Zone::Deck,
        ZoneKind::Hand => Zone::Hand,
        ZoneKind::Field => Zone::Field,
        ZoneKind::Ex => Zone::Ex,
        ZoneKind::Cemetery => Zone::Cemetery,
        ZoneKind::Banish => Zone::Banish,
        ZoneKind::Evolve => Zone::EvolveDeck,
        ZoneKind::Resolution => Zone::Resolution,
        ZoneKind::Race => Zone::Race,
        ZoneKind::Drive => Zone::Drive,
        ZoneKind::Trigger => Zone::Trigger,
        ZoneKind::Equipment => Zone::Equipment,
    }
}
