//! Frozen event payloads supply typed choice details, independently of display labels.

use super::PendingOccurrence;
use crate::game::player::observation::viewer;
use crate::game::player::{PendingEvent, PendingEventContext, Viewer};
use crate::ir::{EntryOrigin, EventPayload, PlayerId, dice, events::fusion::material_filter};

fn event_viewer(player: PlayerId) -> Option<Viewer> {
    viewer(super::super::seat_name(player).ok()?).ok()
}

pub(super) fn player_event_context(pending: &PendingOccurrence) -> Option<PendingEventContext> {
    let (event, positioned) = match pending.timing.as_ref() {
        Some(EventPayload::DriveGained { .. }) => (PendingEvent::DriveGained, true),
        Some(EventPayload::Race { event }) => (
            PendingEvent::Race {
                ordinal: event.ordinal.0,
            },
            true,
        ),
        Some(EventPayload::EnterField { event }) => (
            PendingEvent::EnterField {
                rules_names: event
                    .characters
                    .names
                    .iter()
                    .map(|name| name.0.clone())
                    .collect(),
                from: match event.from {
                    EntryOrigin::Zone { zone } => Some(zone),
                    EntryOrigin::Other {} => None,
                },
            },
            true,
        ),
        Some(EventPayload::EnterEx { .. }) => (PendingEvent::EnterEx, true),
        Some(EventPayload::StatChange { .. }) => (PendingEvent::StatChange, true),
        Some(EventPayload::Posture { event }) => (
            PendingEvent::Posture {
                posture: event.after,
            },
            true,
        ),
        Some(EventPayload::Destroy { .. }) => (PendingEvent::Destroy, true),
        Some(EventPayload::UbActivated { .. }) => (PendingEvent::UnionBurst, true),
        Some(EventPayload::Fused { .. }) if material_filter(&pending.ability.node).is_some() => {
            (PendingEvent::FusedAsMaterial, false)
        }
        Some(EventPayload::Fused { .. }) => (PendingEvent::Fusion, true),
        Some(EventPayload::Play { .. }) => (PendingEvent::Play, true),
        Some(EventPayload::Discard { .. }) => (PendingEvent::Discard, false),
        Some(EventPayload::Banish { .. }) => (PendingEvent::Banish, false),
        Some(EventPayload::Draw { .. }) => (PendingEvent::Draw, false),
        Some(EventPayload::DriveChecked { .. }) => (PendingEvent::DriveChecked, false),
        Some(EventPayload::DriveTrigger { .. }) => (PendingEvent::DriveTrigger, false),
        Some(EventPayload::Dice { event }) => (
            PendingEvent::Dice {
                value: dice::value(event.value),
            },
            false,
        ),
        Some(EventPayload::Equip { .. }) => (PendingEvent::Equip, true),
        Some(EventPayload::Move { event }) => (
            PendingEvent::Move {
                player: event_viewer(event.from.player)?,
                from: event.from.kind,
                to: event.to.kind,
            },
            true,
        ),
        Some(EventPayload::Attack { event }) => (
            PendingEvent::Attack {
                ordinal: event.ordinal.0,
            },
            true,
        ),
        Some(EventPayload::Phase { event }) => (
            PendingEvent::Phase {
                phase: event.phase,
                turn: event.turn.0,
                player: event_viewer(event.player)?,
            },
            false,
        ),
        Some(EventPayload::Evolve { event }) => (PendingEvent::Evolve { state: event.kind }, true),
        Some(EventPayload::LifeChange { event }) => (
            PendingEvent::LifeChange {
                delta: event.delta.0,
                player: event_viewer(event.subject)?,
            },
            true,
        ),
        Some(EventPayload::Damage { event }) => (
            PendingEvent::Damage {
                amount: event.amount.0.get(),
            },
            true,
        ),
        Some(EventPayload::LeaveField { .. }) | None if pending.leave.is_some() => {
            (PendingEvent::LeaveField, true)
        }
        _ => return None,
    };
    Some(PendingEventContext {
        event,
        position: positioned.then_some(pending.source_position),
    })
}
