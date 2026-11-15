//! Native player events as wire event bodies.
//!
//! Committed history and provisional presentation project the same native result through
//! this one function, so a promoted event carries exactly the body its scope showed.

mod feed;

pub use feed::{EventFeed, FeedUpdate};

use super::ProjectionError;
use crate::game::player::Viewer;
use crate::game::player::events::{
    ActionDetails, ActionEvent, ActionKind, EventCard, EventKind, EventLocation, EventSubject,
    MovementMethod, NumberFacet,
};
use crate::game::player::observation::{EffectSource, ObservedZone, TurnObservation};
use crate::ir::{
    AbilityQualification, DamageFlags, EvolutionState, MoveMethod, PaymentTime, Stat, TriggerIcon,
};
use sve_battle_contract::event::{self as wire, EventBody};
use sve_battle_contract::observation::{self, Facing, Posture, TurnView};
use sve_battle_contract::question;
use sve_battle_contract::{Int, Nat, Seat, SourceRef, Subject, Zone};

/// Identities and text that the board projection of the same viewer supplies, so that an
/// event names a card exactly as that viewer's board does.
pub trait EventObjects {
    /// The wire card for a card as an event saw it: its authorized face and, only while the
    /// viewer could identify it, its object.
    ///
    /// # Errors
    /// The face or object cannot be represented safely.
    fn card(&self, card: &EventCard) -> Result<wire::EventCard, ProjectionError>;

    /// The wire text origin of a publicly identified effect.
    ///
    /// # Errors
    /// The origin cannot be represented safely.
    fn source(&self, source: &EffectSource) -> Result<SourceRef, ProjectionError>;

    /// The wire form of a public turn state.
    ///
    /// # Errors
    /// A number exceeds the wire range.
    fn turn(&self, turn: &TurnObservation) -> Result<TurnView, ProjectionError>;
}

/// One wire event body and the subjects a client may highlight for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedEvent {
    /// What happened.
    pub body: EventBody,
    /// Leaders and identifiable cards the body names, in order, without repeats.
    pub about: Vec<Subject>,
}

/// Project every result of one complete batch, in order.
///
/// A batch is projected as a whole because a shared ending is a property of the batch: one
/// player's own result does not say that the other player lost or won with them. The caller
/// must pass a batch exactly as its producer sealed it; a shared cause or group proves
/// nothing.
///
/// # Errors
/// A result needs a fact the native producer did not provide, or a number exceeds the wire
/// range; nothing is returned for the batch then.
pub fn project_batch<'kinds, K, O>(
    kinds: K,
    objects: &O,
) -> Result<Vec<ProjectedEvent>, ProjectionError>
where
    K: IntoIterator<Item = &'kinds EventKind> + Clone,
    O: EventObjects,
{
    let ending = Ending {
        winners: kinds
            .clone()
            .into_iter()
            .filter(|kind| {
                matches!(
                    kind,
                    EventKind::Result {
                        winner: Some(_),
                        ..
                    }
                )
            })
            .count(),
        losers: kinds
            .clone()
            .into_iter()
            .filter(|kind| matches!(kind, EventKind::Result { loser: Some(_), .. }))
            .count(),
    };
    kinds
        .into_iter()
        .map(|kind| {
            let mut projection = Projection {
                objects,
                about: Vec::new(),
            };
            let body = projection.body(kind, ending)?;
            Ok(ProjectedEvent {
                body,
                about: projection.about,
            })
        })
        .collect()
}

/// How many players the results of one batch name as winning and as losing.
#[derive(Clone, Copy)]
struct Ending {
    winners: usize,
    losers: usize,
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

const fn opponent(viewer: Viewer) -> Viewer {
    match viewer {
        Viewer::P1 => Viewer::P2,
        Viewer::P2 => Viewer::P1,
    }
}

const fn zone(zone: ObservedZone) -> Zone {
    match zone {
        ObservedZone::Deck => Zone::Deck,
        ObservedZone::EvolveDeck => Zone::EvolveDeck,
        ObservedZone::Hand => Zone::Hand,
        ObservedZone::Field => Zone::Field,
        ObservedZone::Ex => Zone::Ex,
        ObservedZone::Cemetery => Zone::Cemetery,
        ObservedZone::Banish => Zone::Banish,
        ObservedZone::Resolution => Zone::Resolution,
        ObservedZone::Evolution => Zone::Evolution,
        ObservedZone::Race => Zone::Race,
        ObservedZone::Drive => Zone::Drive,
        ObservedZone::Trigger => Zone::Trigger,
        ObservedZone::Equipment => Zone::Equipment,
    }
}

/// The wire method and whether an effect instruction, rather than a rule or procedure,
/// performed the move. The native producer states which: an effect's draw is recorded as
/// `Effect(Draw)`, so a bare `Draw` is always a rule's.
const fn method(method: MovementMethod) -> (wire::MovementMethod, bool) {
    match method {
        MovementMethod::Effect(MoveMethod::Put) => (wire::MovementMethod::Put, true),
        MovementMethod::Effect(MoveMethod::Return) => (wire::MovementMethod::Return, true),
        MovementMethod::Effect(MoveMethod::Mill) => (wire::MovementMethod::Mill, true),
        MovementMethod::Effect(MoveMethod::Draw) => (wire::MovementMethod::Draw, true),
        MovementMethod::Effect(MoveMethod::Discard) => (wire::MovementMethod::Discard, true),
        MovementMethod::Effect(MoveMethod::Destroy) => (wire::MovementMethod::Destroy, true),
        MovementMethod::Effect(MoveMethod::Banish) => (wire::MovementMethod::Banish, true),
        MovementMethod::Effect(MoveMethod::Play) => (wire::MovementMethod::Play, true),
        MovementMethod::Draw => (wire::MovementMethod::Draw, false),
        MovementMethod::Discard => (wire::MovementMethod::Discard, false),
        MovementMethod::Procedure => (wire::MovementMethod::Procedure, false),
        MovementMethod::RuleDestroy => (wire::MovementMethod::Destroy, false),
    }
}

const fn damage_class(flags: DamageFlags) -> wire::DamageClass {
    match flags {
        DamageFlags::Ability => wire::DamageClass::Ability,
        DamageFlags::Attack => wire::DamageClass::Attack,
        DamageFlags::Combat => wire::DamageClass::Combat,
        DamageFlags::AttackCombat => wire::DamageClass::AttackCombat,
    }
}

const fn evolution(state: EvolutionState) -> observation::EvolutionState {
    match state {
        EvolutionState::Base => observation::EvolutionState::Base,
        EvolutionState::Evolved => observation::EvolutionState::Evolved,
        EvolutionState::SuperEvolved => observation::EvolutionState::SuperEvolved,
    }
}

const fn icon(icon: TriggerIcon) -> question::TriggerIcon {
    match icon {
        TriggerIcon::Critical => question::TriggerIcon::Critical,
        TriggerIcon::Draw => question::TriggerIcon::Draw,
        TriggerIcon::Stand => question::TriggerIcon::Stand,
        TriggerIcon::Heal => question::TriggerIcon::Heal,
        TriggerIcon::Front => question::TriggerIcon::Front,
    }
}

fn int(value: i64) -> Result<Int, ProjectionError> {
    Ok(Int::try_from(value)?)
}

fn nat(value: i64) -> Result<Nat, ProjectionError> {
    Ok(Nat::try_from(value)?)
}

fn change(facet: &NumberFacet) -> Result<wire::ValueChange, ProjectionError> {
    Ok(wire::ValueChange {
        before: facet.before.map(int).transpose()?,
        after: facet.after.map(int).transpose()?,
        delta: int(facet.amount)?,
    })
}

/// The seat a result names; a wire body that requires it cannot be guessed from the board.
fn player(action: &ActionEvent) -> Result<Seat, ProjectionError> {
    action.player.map(seat).ok_or(ProjectionError::Unavailable)
}

fn number(action: &ActionEvent) -> Result<&NumberFacet, ProjectionError> {
    action.numbers.first().ok_or(ProjectionError::Unavailable)
}

fn slot(action: &ActionEvent, index: usize) -> Option<&EventSubject> {
    action.subjects.get(index).and_then(Option::as_ref)
}

struct Projection<'objects, O> {
    objects: &'objects O,
    about: Vec<Subject>,
}

impl<O: EventObjects> Projection<'_, O> {
    fn note(&mut self, subject: &Subject) {
        if !self.about.contains(subject) {
            self.about.push(subject.clone());
        }
    }

    fn card(&mut self, card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        let projected = self.objects.card(card)?;
        // A remembered face has no object left to highlight.
        if projected.object.is_some() {
            self.note(&Subject::Card {
                card: projected.card.clone(),
                object: projected.object.clone(),
            });
        }
        Ok(projected)
    }

    fn subject(
        &mut self,
        subject: Option<&EventSubject>,
    ) -> Result<Option<Subject>, ProjectionError> {
        match subject {
            None => Ok(None),
            Some(EventSubject::Leader(viewer)) => {
                let leader = Subject::Leader {
                    player: seat(*viewer),
                };
                self.note(&leader);
                Ok(Some(leader))
            }
            Some(EventSubject::Card(card)) => {
                let projected = self.card(card)?;
                Ok(Some(Subject::Card {
                    card: projected.card,
                    object: projected.object,
                }))
            }
        }
    }

    /// A participant that the wire names as a card; a leader in that role is a producer gap.
    fn event_card(
        &mut self,
        subject: Option<&EventSubject>,
    ) -> Result<Option<wire::EventCard>, ProjectionError> {
        match subject {
            None => Ok(None),
            Some(EventSubject::Card(card)) => self.card(card).map(Some),
            Some(EventSubject::Leader(_)) => Err(ProjectionError::Unavailable),
        }
    }

    fn endpoint(&mut self, location: &EventLocation) -> Result<wire::Endpoint, ProjectionError> {
        Ok(wire::Endpoint {
            player: seat(location.player),
            zone: location.zone.map(zone),
            position: location.position.map(Nat::try_from).transpose()?,
            card: location
                .card
                .as_ref()
                .map(|card| self.card(card))
                .transpose()?,
        })
    }

    fn source(&self, action: &ActionEvent) -> Result<Option<SourceRef>, ProjectionError> {
        action
            .source
            .as_ref()
            .map(|source| self.objects.source(source))
            .transpose()
    }

    fn body(&mut self, kind: &EventKind, ending: Ending) -> Result<EventBody, ProjectionError> {
        match kind {
            EventKind::Movement {
                method: moved,
                before,
                after,
            } => {
                let (method, by_effect) = method(*moved);
                Ok(EventBody::Movement {
                    method,
                    by_effect,
                    before: self.endpoint(before)?,
                    after: self.endpoint(after)?,
                })
            }
            EventKind::Action(action) => self.action(action),
            EventKind::Phase(turn) => Ok(EventBody::PhaseStarted {
                turn: self.objects.turn(turn)?,
            }),
            EventKind::Result {
                winner,
                loser,
                rule,
            } => result(*winner, *loser, rule.as_deref(), ending),
        }
    }

    /// One arm per native kind; each wire body is built by its own small function so that
    /// a missing producer fact is refused where it is needed.
    fn action(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        match action.kind {
            ActionKind::Play => self.card_played(action),
            ActionKind::Resolve => self.card_resolved(action),
            ActionKind::AbilityPlay => self.ability_played(action),
            ActionKind::AbilityResolve => self.ability_resolved(action),
            ActionKind::Attack => self.attack(action),
            ActionKind::Damage => self.damage(action),
            ActionKind::Life => life(action),
            ActionKind::Stats => self.stats(action),
            ActionKind::Resource => self.resource(action),
            ActionKind::Counter => self.counter(action),
            ActionKind::Act => self.posture(action, Posture::Acted),
            ActionKind::Stand => self.posture(action, Posture::Standing),
            ActionKind::Reveal => self.revealed(action),
            ActionKind::Facing => self.facing(action),
            ActionKind::Destroy => self
                .subject(slot(action, 0))
                .map(|subject| EventBody::DestroyAttempted { subject }),
            ActionKind::Banish => self
                .subject(slot(action, 0))
                .map(|subject| EventBody::BanishAttempted { subject }),
            ActionKind::Replacement => self.replacement(action),
            ActionKind::Erase => self
                .subject(slot(action, 0))
                .map(|subject| EventBody::Erased { subject }),
            ActionKind::Equip => self.equipped(action),
            ActionKind::Evolve => self.evolved(action, wire::EvolutionRole::Normal),
            ActionKind::EffectEvolve => self.evolved(action, wire::EvolutionRole::Effect),
            ActionKind::Fusion => self.fusion(action),
            ActionKind::Race => self.race(action),
            ActionKind::DriveGained => self.drive_gained(action),
            ActionKind::DriveTrigger => self.drive_trigger(action),
            ActionKind::DriveChecked => self.drive_checked(action),
            ActionKind::Payment => payment(action),
            ActionKind::Dice => dice(action),
            ActionKind::Declare => declared(action),
            ActionKind::Shuffle => shuffled(action),
            ActionKind::TurnSkipped => turn_skipped(action),
            // The native trace keeps neither the owner nor the two controllers.
            ActionKind::Control => Err(ProjectionError::Unavailable),
        }
    }

    fn card_played(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::CardPlayed {
            player: player(action)?,
            card: self.event_card(slot(action, 0))?,
        })
    }

    fn card_resolved(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::CardResolved {
            player: player(action)?,
            card: self.event_card(slot(action, 0))?,
        })
    }

    fn ability_played(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::Ability(qualification)) = &action.details else {
            return Err(ProjectionError::Unavailable);
        };
        Ok(EventBody::AbilityPlayed {
            player: player(action)?,
            holder: self.subject(slot(action, 0))?,
            source: self.source(action)?,
            // No native producer states the ability's category.
            ability: None,
            qualification: match qualification {
                AbilityQualification::Ordinary => wire::AbilityQualification::Ordinary,
                AbilityQualification::Fusion => wire::AbilityQualification::Fusion,
                AbilityQualification::UnionBurst => wire::AbilityQualification::UnionBurst,
            },
        })
    }

    fn ability_resolved(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::AbilityResolved {
            player: player(action)?,
            holder: self.subject(slot(action, 0))?,
            source: self.source(action)?,
            ability: None,
        })
    }

    fn attack(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::AttackDeclared {
            attacker: self.subject(slot(action, 0))?,
            target: self.subject(slot(action, 1))?,
        })
    }

    fn damage(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::Damage(flags)) = &action.details else {
            return Err(ProjectionError::Unavailable);
        };
        let dealt = number(action)?;
        Ok(EventBody::Damage {
            source: self.subject(slot(action, 0))?,
            target: self.subject(slot(action, 1))?,
            class: damage_class(*flags),
            amount: nat(dealt.amount)?,
            life_after: dealt.after.map(int).transpose()?,
        })
    }

    fn stats(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::StatsChanged {
            subject: self.subject(slot(action, 0))?,
            changes: action
                .numbers
                .iter()
                .map(|facet| {
                    Ok(wire::StatChange {
                        stat: match facet.stat.ok_or(ProjectionError::Unavailable)? {
                            Stat::Attack => wire::Stat::Attack,
                            Stat::Life => wire::Stat::Life,
                        },
                        change: change(facet)?,
                    })
                })
                .collect::<Result<_, ProjectionError>>()?,
        })
    }

    fn counter(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::CounterChanged {
            subject: self.subject(slot(action, 0))?,
            name: action
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .ok_or(ProjectionError::Unavailable)?,
            change: change(number(action)?)?,
        })
    }

    fn posture(
        &mut self,
        action: &ActionEvent,
        posture: Posture,
    ) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::PostureChanged {
            subject: self.subject(slot(action, 0))?,
            posture,
        })
    }

    fn revealed(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::Revealed {
            cards: self.event_card(slot(action, 0))?.into_iter().collect(),
        })
    }

    fn facing(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::FacingChanged {
            subject: self.subject(slot(action, 0))?,
            facing: if action.face_up.ok_or(ProjectionError::Unavailable)? {
                Facing::Up
            } else {
                Facing::Down
            },
        })
    }

    fn replacement(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::ReplacementApplied {
            subject: self.subject(slot(action, 0))?,
            source: self.source(action)?,
        })
    }

    /// The holder is recorded first and the equipment second.
    fn equipped(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::Equipped {
            equipment: self.subject(slot(action, 1))?,
            bearer: self.subject(slot(action, 0))?,
        })
    }

    fn fusion(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::Fusion(traits)) = &action.details else {
            return Err(ProjectionError::Unavailable);
        };
        Ok(EventBody::Fusion {
            player: player(action)?,
            subject: self.subject(slot(action, 0))?,
            materials: nat(number(action)?.amount)?,
            material_traits: traits.clone(),
        })
    }

    fn race(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::Race {
            player: player(action)?,
            subject: self.subject(slot(action, 0))?,
            ordinal: nat(number(action)?.amount)?,
        })
    }

    fn drive_gained(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        Ok(EventBody::DriveGained {
            player: player(action)?,
            subject: self.subject(slot(action, 0))?,
        })
    }

    fn drive_trigger(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::DriveTrigger(found)) = &action.details else {
            return Err(ProjectionError::Unavailable);
        };
        Ok(EventBody::DriveTrigger {
            player: player(action)?,
            card: self.event_card(slot(action, 0))?,
            icon: icon(*found),
        })
    }

    fn drive_checked(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::DriveChecked {
            accepted,
            player: owner,
            zone: destination,
        }) = &action.details
        else {
            return Err(ProjectionError::Unavailable);
        };
        Ok(EventBody::DriveChecked {
            player: player(action)?,
            card: self.event_card(slot(action, 0))?,
            accepted: *accepted,
            destination: wire::ZoneRef {
                player: seat(*owner),
                zone: zone(*destination),
            },
        })
    }

    fn resource(&mut self, action: &ActionEvent) -> Result<EventBody, ProjectionError> {
        let resource = match action.name.as_deref() {
            Some("pp") => wire::Resource::Pp,
            Some("maximum_pp") => wire::Resource::MaxPp,
            Some("ep") => wire::Resource::Ep,
            Some("life") => wire::Resource::Life,
            Some(_) | None => return Err(ProjectionError::Unavailable),
        };
        // A leader's life gain names the leader rather than a paying seat.
        let leader = match slot(action, 0) {
            Some(named @ EventSubject::Leader(viewer)) => {
                self.subject(Some(named))?;
                Some(*viewer)
            }
            Some(EventSubject::Card(_)) | None => None,
        };
        let owner = action
            .player
            .or(leader)
            .map(seat)
            .ok_or(ProjectionError::Unavailable)?;
        Ok(EventBody::ResourceChanged {
            player: owner,
            resource,
            change: change(number(action)?)?,
        })
    }

    fn evolved(
        &mut self,
        action: &ActionEvent,
        role: wire::EvolutionRole,
    ) -> Result<EventBody, ProjectionError> {
        let Some(ActionDetails::Evolution(state)) = &action.details else {
            return Err(ProjectionError::Unavailable);
        };
        Ok(EventBody::Evolved {
            subject: self.subject(slot(action, 0))?,
            form: self.event_card(slot(action, 1))?,
            state: evolution(*state),
            role,
        })
    }
}

fn life(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    Ok(EventBody::LifeChanged {
        player: player(action)?,
        change: change(number(action)?)?,
    })
}

fn dice(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    Ok(EventBody::Dice {
        player: player(action)?,
        value: nat(number(action)?.amount)?,
    })
}

/// The number an effect had its player declare; it is not the X of a cost.
fn declared(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    Ok(EventBody::ChoiceDeclared {
        player: player(action)?,
        declared: observation::Declared::Number {
            role: observation::DeclaredNumber::Free,
            value: int(number(action)?.amount)?,
        },
    })
}

/// Both native producers shuffle a main deck; no other zone is ever shuffled.
fn shuffled(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    Ok(EventBody::Shuffled {
        player: player(action)?,
        zone: Zone::Deck,
    })
}

fn turn_skipped(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    let Some(ActionDetails::TurnSkipped { extra }) = &action.details else {
        return Err(ProjectionError::Unavailable);
    };
    Ok(EventBody::TurnSkipped {
        player: player(action)?,
        extra: *extra,
    })
}

fn payment(action: &ActionEvent) -> Result<EventBody, ProjectionError> {
    let Some(ActionDetails::Payment {
        time,
        complete,
        zero,
        replaced,
    }) = &action.details
    else {
        return Err(ProjectionError::Unavailable);
    };
    // Only an Earth Rite payment names what was paid; its removed stacks are the number.
    let component = match (&action.name, action.numbers.first()) {
        (Some(keyword), Some(stacks)) => wire::PaymentComponent::EarthRite {
            keyword: keyword.clone(),
            stacks: nat(stacks.amount)?,
        },
        (None, None) => wire::PaymentComponent::General,
        (Some(_), None) | (None, Some(_)) => return Err(ProjectionError::Unavailable),
    };
    Ok(EventBody::Payment {
        player: player(action)?,
        time: match time {
            PaymentTime::PlayParameter => wire::PaymentTime::PlayParameter,
            PaymentTime::ResolutionChoice => wire::PaymentTime::ResolutionChoice,
        },
        complete: *complete,
        zero: *zero,
        replaced: *replaced,
        component,
    })
}

/// A native result names either one player an effect made win or one player who lost; the
/// other results of its batch say whether that player won or lost alone.
fn result(
    winner: Option<Viewer>,
    loser: Option<Viewer>,
    rule: Option<&str>,
    ending: Ending,
) -> Result<EventBody, ProjectionError> {
    // No producer ends a game with a win and a defeat at once; their winners could disagree.
    if ending.winners > 0 && ending.losers > 0 {
        return Err(ProjectionError::Unavailable);
    }
    let (winner, reason) = match (winner, loser) {
        (Some(won), None) if ending.winners == 1 => (Some(won), wire::ResultReason::Effect),
        // An effect made both players win together: the game is drawn.
        (Some(_), None) => (None, wire::ResultReason::Effect),
        // Both players lost together: nobody won.
        (None, Some(_)) if ending.losers > 1 => (None, wire::ResultReason::SimultaneousDefeat),
        (None, Some(lost)) if rule == Some("rule-1.2.3") => {
            (Some(opponent(lost)), wire::ResultReason::Concession)
        }
        (None, Some(lost)) => (Some(opponent(lost)), wire::ResultReason::Defeat),
        (Some(_), Some(_)) | (None, None) => return Err(ProjectionError::Unavailable),
    };
    Ok(EventBody::Result {
        winner: winner.map(seat),
        loser: loser.map(seat),
        reason,
        // The native trace keeps a clause number without the revision a rule reference needs.
        rule: None,
    })
}

#[cfg(test)]
mod tests;
