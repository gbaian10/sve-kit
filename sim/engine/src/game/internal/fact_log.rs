//! Facts are recorded at production; moving an already recorded child prefix is not production.

use super::{Execution, Fact, Game};
use crate::game::player::Viewer;
use crate::game::player::events::{
    ActionDetails, ActionEvent, ActionKind, EventError, EventKind, EventSubject, NumberFacet,
};
use crate::game::player::observation::EffectSource;
use crate::game::player::observation::ObservedZone;
use crate::ir::ZoneKind;
use crate::ir::{ObjectRef, PaidComponent, PlayerId, RuntimeHolder};
use core::iter::once;
use core::ops::Deref;
use core::slice::Iter;

#[derive(Clone, Debug, Default)]
pub(super) struct Facts(Vec<Fact>);

impl Deref for Facts {
    type Target = [Fact];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'facts> IntoIterator for &'facts Facts {
    type Item = &'facts Fact;
    type IntoIter = Iter<'facts, Fact>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Facts {
    pub(super) fn append_recorded(&mut self, other: &mut Self) {
        self.0.append(&mut other.0);
    }
}

const fn viewer(player: PlayerId) -> Option<Viewer> {
    match player.seat {
        1 => Some(Viewer::P1),
        2 => Some(Viewer::P2),
        _ => None,
    }
}

const fn number(amount: i64) -> NumberFacet {
    NumberFacet {
        stat: None,
        before: None,
        after: None,
        amount,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Fact production shares the existing execution."
)]
impl Execution {
    pub(super) fn record_fact(&mut self, game: &mut Game, fact: Fact) {
        if game.player_events.active() {
            let views = [Viewer::P1, Viewer::P2]
                .map(|perspective| self.project_fact(game, perspective, &fact));
            let group = match &fact {
                Fact::StatChange(change) => Some(format!("stat/{}/{}", self.id.0, change.effect.0)),
                _ => None,
            };
            game.record_player_fact_views(views, &self.frame().cause, group);
        }
        self.facts.0.push(fact);
    }

    fn event_member(
        &self,
        game: &Game,
        perspective: Viewer,
        member: ObjectRef,
    ) -> Result<Option<EventSubject>, EventError> {
        let id = self
            .object_id(member)
            .map_err(|_authority_error| EventError::ProjectionGap)?;
        // A later incarnation or a newly disclosed hidden card is never an LKI identity proof.
        if !game
            .object(id)
            .is_ok_and(|object| object.generation == member.generation.0)
        {
            return Ok(game.player_event_memory(perspective, id, member.generation.0));
        }
        game.player_event_subject(perspective, id)
    }

    fn event_source(&self, game: &Game) -> Result<Option<EffectSource>, EventError> {
        if self.ability.is_none() {
            return Ok(None);
        }
        let public = self
            .source_information
            .as_ref()
            .is_some_and(super::source::SourceInformation::publicly_identified);
        if !public {
            return Ok(None);
        }
        let provider = game
            .installed_effect_provider(self)
            .map_err(|_authority_error| EventError::ProjectionGap)?;
        Ok(Some(EffectSource {
            card: provider.card,
            face: Some(provider.span.face),
            line: Some(u64::from(provider.span.line)),
            holder: None,
        }))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "One exhaustive match owns the union with shared primitive events."
    )]
    fn project_fact(
        &self,
        game: &Game,
        perspective: Viewer,
        fact: &Fact,
    ) -> Result<Option<EventKind>, EventError> {
        let mut action = ActionEvent {
            kind: ActionKind::Life,
            player: None,
            subjects: Vec::new(),
            source: None,
            numbers: Vec::new(),
            name: None,
            face_up: None,
            details: None,
        };
        match fact {
            // These facts' physical results are emitted once by their shared primitive adapters.
            Fact::CardPlayed { .. }
            | Fact::CardResolved { .. }
            | Fact::Equipped(_)
            | Fact::Discard(_)
            | Fact::Posture(_)
            | Fact::Draw(_)
            | Fact::Damage(_)
            | Fact::Dice(_)
            | Fact::Movement(_)
            | Fact::Evolved(_) => return Ok(None),
            Fact::LifeChange(event) => {
                action.kind = ActionKind::Life;
                action.player = viewer(event.subject);
                action.numbers.push(NumberFacet {
                    stat: None,
                    before: Some(event.before.0),
                    after: Some(event.after.0),
                    amount: event.delta.0,
                });
            }
            Fact::StatChange(fact) => {
                action.kind = ActionKind::Stats;
                action.subjects.push(match fact.event.subject {
                    RuntimeHolder::Object { object } => {
                        self.event_member(game, perspective, object)?
                    }
                    RuntimeHolder::Player { player } => viewer(player).map(EventSubject::Leader),
                });
                action.numbers = once(&fact.event.facets.first)
                    .chain(&fact.event.facets.rest)
                    .map(|facet| NumberFacet {
                        stat: Some(facet.property),
                        before: Some(facet.before.0),
                        after: Some(facet.after.0),
                        amount: facet.delta.0,
                    })
                    .collect();
            }
            Fact::Shuffled { player } => {
                action.kind = ActionKind::Shuffle;
                action.player = viewer(*player);
            }
            Fact::Fused(event) => {
                action.kind = ActionKind::Fusion;
                action.details = Some(ActionDetails::Fusion(
                    event
                        .material_traits
                        .iter()
                        .map(|value| value.0.clone())
                        .collect(),
                ));
                action.player = viewer(event.controller);
                action
                    .subjects
                    .push(self.event_member(game, perspective, event.subject_after)?);
                action.numbers.push(number(
                    i64::try_from(event.materials.len())
                        .map_err(|_authority_error| EventError::ProjectionGap)?,
                ));
            }
            Fact::Raced(event) => {
                action.kind = ActionKind::Race;
                action.player = viewer(event.controller);
                action
                    .subjects
                    .push(self.event_member(game, perspective, event.subject)?);
                action.numbers.push(number(
                    i64::try_from(event.ordinal.0)
                        .map_err(|_authority_error| EventError::ProjectionGap)?,
                ));
            }
            Fact::DriveGained(event) => {
                action.kind = ActionKind::DriveGained;
                action.player = viewer(event.controller);
                action
                    .subjects
                    .push(self.event_member(game, perspective, event.subject)?);
            }
            Fact::DriveTrigger(event) => {
                action.kind = ActionKind::DriveTrigger;
                action.details = Some(ActionDetails::DriveTrigger(event.icon));
                action.player = viewer(event.player);
                action
                    .subjects
                    .push(self.event_member(game, perspective, event.checked_card)?);
            }
            Fact::DriveChecked(event) => {
                action.kind = ActionKind::DriveChecked;
                action.details = Some(ActionDetails::DriveChecked {
                    accepted: event.trigger_accepted.0,
                    player: viewer(event.final_destination.player)
                        .ok_or(EventError::ProjectionGap)?,
                    zone: match event.final_destination.kind {
                        ZoneKind::Deck => ObservedZone::Deck,
                        ZoneKind::Cemetery => ObservedZone::Cemetery,
                        ZoneKind::Banish => ObservedZone::Banish,
                        _ => return Err(EventError::ProjectionGap),
                    },
                });
                action.player = viewer(event.player);
                action
                    .subjects
                    .push(self.event_member(game, perspective, event.checked_card)?);
            }
            Fact::Payment { player, event } => {
                action.kind = ActionKind::Payment;
                action.details = Some(ActionDetails::Payment {
                    time: event.phase,
                    complete: event.payment.complete.0,
                    zero: event.zero.0,
                    replaced: event.replaced.0,
                });
                action.player = viewer(*player);
                match event.component {
                    PaidComponent::EarthRite { paid } => {
                        action.name = Some(game.catalog.keyword_name("earth_rite").into());
                        action.numbers.push(number(
                            i64::try_from(paid.removed_stack.0)
                                .map_err(|_authority_error| EventError::ProjectionGap)?,
                        ));
                    }
                    PaidComponent::General {} => {}
                }
            }
            Fact::AbilityPlayed {
                source,
                qualification,
                ..
            } => {
                action.details = Some(ActionDetails::Ability(*qualification));
                action.kind = ActionKind::AbilityPlay;
                action.player = viewer(source.controller);
                action
                    .subjects
                    .push(self.event_member(game, perspective, source.holder)?);
            }
            Fact::AbilityResolved { source, .. } => {
                action.kind = ActionKind::AbilityResolve;
                action.player = viewer(source.controller);
                action
                    .subjects
                    .push(self.event_member(game, perspective, source.holder)?);
            }
        }
        action.source = self.event_source(game)?;
        Ok(Some(EventKind::Action(action)))
    }
}
