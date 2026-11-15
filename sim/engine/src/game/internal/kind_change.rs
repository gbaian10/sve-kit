//! Kind changes leave numeric layers and all ability instances attached to the same generation.
use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Ability, AbilityKind, AbilityOrigin, AbilityQualification, Action, CardKind,
    CharacteristicPatch, DurationEnd, Effect, EffectId, EffectReference, Instruction, Located,
    ObjectOperand, ObjectRef, PlayerRef, TimeAnchor, ZoneKind, kind_change,
};

#[derive(Clone, Debug)]
pub(super) struct Installed {
    id: String,
    subject: ObjectRef,
    kind: CardKind,
    end: EffectEnd,
}

pub(super) fn check_static(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.qualification != AbilityQualification::Ordinary
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static kind header"));
    }
    for effect in declarations {
        let Effect::Continuous { source, .. } = effect else {
            return Err(unsupported("kind declaration"));
        };
        if source != &ability.source || kind_change::projection(effect).is_none() {
            return Err(unsupported("static kind declaration"));
        }
    }
    Ok(())
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Modify {
        patch:
            CharacteristicPatch::SetKind {
                kind: CardKind::Follower | CardKind::Amulet,
            },
        expires,
        source: EffectReference::CurrentInstallation {},
        ..
    } = action
    else {
        return Err(unsupported("kind modification"));
    };
    if expires.anchor != (TimeAnchor::Installation {})
        || expires.usage.is_some()
        || !expires.intervals.is_empty()
        || !matches!(
            expires.end,
            DurationEnd::NoTemporalExpiry {} | DurationEnd::CurrentTurnEnd {}
        )
    {
        return Err(unsupported("kind lifetime"));
    }
    Ok(())
}

fn name(kind: CardKind, token: bool) -> Result<&'static str> {
    match (kind, token) {
        (CardKind::Follower, false) => Ok("フォロワー"),
        (CardKind::Follower, true) => Ok("フォロワー・トークン"),
        (CardKind::Amulet, false) => Ok("アミュレット"),
        (CardKind::Amulet, true) => Ok("アミュレット・トークン"),
        _ => Err(unsupported("field kind")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Every kind query and installation shares the authoritative Game."
)]
impl Game {
    pub(in crate::game) fn internal_effective_kind(
        &self,
        id: &str,
    ) -> Result<Option<&'static str>> {
        let object = self.object(id)?;
        if object.zone != "field" || self.card_runtime.is_empty() {
            return Ok(None);
        }
        let token = self.face(id)?["card_type"]
            .as_str()
            .is_some_and(|kind| kind.contains("トークン"));
        for effect in self.card_runtime.kind_changes.iter().rev() {
            if effect.id == id
                && effect.subject.generation.0 == object.generation
                && !effect.end.source_expired(self)
                && !matches!(effect.end, EffectEnd::Turn(turn) if self.state.schedule.current().is_ok_and(|now| now != turn))
            {
                return name(effect.kind, token).map(Some);
            }
        }
        let (card, _) = self.internal_effective_print(id)?;
        if !self.card_runtime.is_loaded(&card) {
            return Ok(None);
        }
        let mut current = None;
        for ability in self.internal_characteristic_abilities(id)? {
            for effect in &ability.node.declarations {
                if let Some((kind, condition)) = kind_change::projection(effect) {
                    let execution = self.provider_execution(id, None)?;
                    if condition
                        .map(|condition| execution.boolean(self, condition))
                        .transpose()?
                        .is_none_or(|value| value.0)
                    {
                        current = Some(name(kind, token)?);
                    }
                }
            }
        }
        Ok(current)
    }

    pub(super) fn internal_modify_kind(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Modify {
            subjects,
            patch: CharacteristicPatch::SetKind { kind },
            expires,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("kind action"));
        };
        let end = EffectEnd::at(self, execution, &expires.end)?;
        let mut planned = Vec::new();
        for subject in execution.objects(self, subjects)? {
            let id = execution.object_id(subject)?;
            let object = self.object(id)?;
            if object.zone == "field" && object.generation == subject.generation.0 {
                planned.push((id.to_owned(), subject));
            }
        }
        let effect = if planned.is_empty() {
            None
        } else {
            let id = EffectId(self.state.next_ir_effect);
            self.state.next_ir_effect = self
                .state
                .next_ir_effect
                .checked_add(1)
                .ok_or_else(|| invalid("kind effect identities exhausted"))?;
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("kind epoch exhausted"))?;
            Some(id)
        };
        for (id, subject) in &planned {
            self.card_runtime.kind_changes.push(Installed {
                id: id.clone(),
                subject: *subject,
                kind: *kind,
                end,
            });
            let current = self.object_type(id)?.to_owned();
            // Later instructions keep referring to the object this execution itself changed.
            for wait in execution.targets.values_mut() {
                wait.accept_kind_change(*subject, &current);
            }
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Modified {
                effect,
                subjects: planned.into_iter().map(|(_, subject)| subject).collect(),
            },
        );
        Ok(())
    }

    pub(super) fn internal_kind_cleanup(&mut self) {
        let objects = &self.state.objects;
        self.card_runtime.kind_changes.retain(|effect| {
            objects.get(&effect.id).is_some_and(|object| {
                object.zone == "field" && object.generation == effect.subject.generation.0
            })
        });
    }

    pub(super) fn internal_kind_end_turn(&mut self, turn: u64) {
        self.card_runtime
            .kind_changes
            .retain(|effect| effect.end != EffectEnd::Turn(turn));
    }
}
