//! One authority observation supplies every board component and its object scope.

use super::{
    BattleReferences, ObjectTable, ProjectionError, costs, effects, history,
    objects::{self, ObjectScope},
    pending, turn,
};
use crate::catalog::Catalog;
use crate::game::player::{ObjectHandle as NativeHandle, Viewer, observation as native};
use alloc::collections::BTreeSet;
use core::result;
use sve_battle_contract::{CardRef, Int, Nat, Seat, Subject, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

pub(crate) struct RuleProcessing {
    pub(crate) stage: wire::ProcessingStage,
    pub(crate) controller: Option<Seat>,
    pub(crate) subject: Option<NativeHandle>,
    pub(crate) procedure: Option<wire::Procedure>,
}

pub(crate) fn build_observation(
    catalog: &Catalog,
    references: &BattleReferences,
    observation: &native::Observation,
    processing: Vec<RuleProcessing>,
) -> Result<wire::BattleObservation> {
    references.validate_catalog(catalog)?;
    let scope = ObjectScope::new(observation)?;
    let objects = objects::build_scoped(catalog, references, observation, &scope)?;
    // Rule subjects share the board's scope and effective faces, just like attack subjects.
    let processing = processing
        .into_iter()
        .map(|context| {
            Ok(wire::ProcessingContext {
                stage: context.stage,
                controller: context.controller,
                source: None,
                subject: context
                    .subject
                    .as_ref()
                    .map(|subject| card_subject(&scope, &objects, subject))
                    .transpose()?,
                rule: None,
                procedure: context.procedure,
                // Evolution parameters have no Declared variant and are not payment receipts.
                declared: vec![],
                payment: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let attack = observation
        .attack
        .as_ref()
        .map(|attack| -> Result<_> {
            Ok(wire::AttackView {
                attacker: card_subject(&scope, &objects, &attack.attacker)?,
                target: match &attack.target {
                    native::EffectRecipient::Leader(player) => Subject::Leader {
                        player: seat(*player),
                    },
                    native::EffectRecipient::Card(card) => card_subject(&scope, &objects, card)?,
                },
            })
        })
        .transpose()?;
    Ok(wire::BattleObservation {
        viewer: seat(observation.viewer),
        objects,
        turn: turn::project_turn(&observation.turn)?,
        attack,
        players: [
            player(catalog, references, &scope, &observation.players[0])?,
            player(catalog, references, &scope, &observation.players[1])?,
        ],
        relationships: observation
            .relationships
            .iter()
            .map(|link| {
                Ok(wire::Relationship {
                    kind: match link.kind {
                        native::RelationshipKind::Evolution => wire::RelationshipKind::Evolution,
                        native::RelationshipKind::Equipment => wire::RelationshipKind::Equipment,
                        native::RelationshipKind::Race => wire::RelationshipKind::Race,
                        native::RelationshipKind::Drive => wire::RelationshipKind::Drive,
                    },
                    from: scope.handle(&link.from)?,
                    to: scope.handle(&link.to)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        processing,
        costs: costs::scoped(observation, &scope)?,
        pending: pending::scoped(catalog, references, observation, &scope)?,
        history: history::scoped(observation, &scope)?,
        remembered: memory(catalog, references, &observation.remembered)?,
    })
}

fn card_subject(
    scope: &ObjectScope<'_>,
    objects: &ObjectTable,
    card: &NativeHandle,
) -> Result<Subject> {
    let object = scope.handle(card)?;
    let view = objects.get(&object).ok_or(ProjectionError::Unavailable)?;
    Ok(Subject::Card {
        card: view.effective_face.clone(),
        object: Some(object),
    })
}

fn player(
    catalog: &Catalog,
    references: &BattleReferences,
    scope: &ObjectScope<'_>,
    player: &native::PlayerObservation,
) -> Result<wire::PlayerView> {
    let resources = &player.resources;
    Ok(wire::PlayerView {
        seat: seat(player.player),
        life: Int::try_from(resources.life)?,
        pp: Int::try_from(resources.pp)?,
        max_pp: Int::try_from(resources.maximum_pp)?,
        ep: Int::try_from(resources.ep)?,
        sep: Int::try_from(resources.sep)?,
        effects: effects::installed(catalog, references, scope, &player.effects)?,
        zones: zones(scope, &player.zones)?,
    })
}

fn zones(scope: &ObjectScope<'_>, observed: &[native::ZoneObservation]) -> Result<wire::Zones> {
    let empty = wire::ZoneView {
        count: Nat::ZERO,
        capacity: None,
        cards: vec![],
    };
    let mut result = wire::Zones {
        deck: empty.clone(),
        evolve_deck: empty.clone(),
        hand: empty.clone(),
        field: empty.clone(),
        ex: empty.clone(),
        cemetery: empty.clone(),
        banish: empty.clone(),
        resolution: empty.clone(),
        evolution: empty.clone(),
        race: empty.clone(),
        drive: empty.clone(),
        trigger: empty.clone(),
        equipment: empty,
    };
    let mut seen = Vec::new();
    for zone in observed {
        if seen.contains(&zone.kind)
            || u64::try_from(zone.cards.len())
                .ok()
                .is_none_or(|n| n > zone.count)
        {
            return Err(ProjectionError::Unavailable);
        }
        seen.push(zone.kind);
        *zone_slot(&mut result, zone.kind) = wire::ZoneView {
            count: Nat::try_from(zone.count)?,
            capacity: zone.capacity.map(Nat::try_from).transpose()?,
            cards: zone
                .cards
                .iter()
                .map(|member| {
                    if member
                        .position
                        .is_some_and(|position| position >= zone.count)
                    {
                        return Err(ProjectionError::Unavailable);
                    }
                    Ok(wire::ZoneMember {
                        position: member.position.map(Nat::try_from).transpose()?,
                        object: scope.handle(&member.card.handle)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
    }
    Ok(result)
}

const fn zone_slot(zones: &mut wire::Zones, zone: native::ObservedZone) -> &mut wire::ZoneView {
    match zone {
        native::ObservedZone::Deck => &mut zones.deck,
        native::ObservedZone::EvolveDeck => &mut zones.evolve_deck,
        native::ObservedZone::Hand => &mut zones.hand,
        native::ObservedZone::Field => &mut zones.field,
        native::ObservedZone::Ex => &mut zones.ex,
        native::ObservedZone::Cemetery => &mut zones.cemetery,
        native::ObservedZone::Banish => &mut zones.banish,
        native::ObservedZone::Resolution => &mut zones.resolution,
        native::ObservedZone::Evolution => &mut zones.evolution,
        native::ObservedZone::Race => &mut zones.race,
        native::ObservedZone::Drive => &mut zones.drive,
        native::ObservedZone::Trigger => &mut zones.trigger,
        native::ObservedZone::Equipment => &mut zones.equipment,
    }
}

fn memory(
    catalog: &Catalog,
    references: &BattleReferences,
    remembered: &[native::CardMemory],
) -> Result<Vec<wire::RememberedCard>> {
    let values = remembered
        .iter()
        .map(|memory| -> Result<(CardRef, Seat)> {
            Ok((
                references.resolve(
                    catalog,
                    &native::CardFace {
                        card: memory.card.clone(),
                        face: memory.face,
                    },
                )?,
                seat(memory.owner),
            ))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    Ok(values
        .into_iter()
        .map(|(card, owner)| wire::RememberedCard { card, owner })
        .collect())
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
