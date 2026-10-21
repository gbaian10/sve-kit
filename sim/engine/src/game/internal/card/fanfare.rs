use super::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, CardPlay, Context, Expr,
    Game, Generation, Located, ObjectRef, PlayerId, Result, RuntimeAbilitySource, Step, ZoneKind,
    face_ordinal, invalid, json, printed_origin, seat_name, take, unsupported,
};
use crate::ir::{
    EventPattern, EventSnapshotPolicy, KeywordId, ObjectFrozen, Predicate, TriggerAggregation,
    TriggerCondition,
};
use crate::ir::{Int, ReceiptId};

#[derive(Clone, Debug)]
pub(super) struct PendingFanfare {
    holder: String,
    source: RuntimeAbilitySource,
    ability: Located<Ability>,
}

pub(super) fn check_header(ability: &Ability) -> Result<()> {
    let Some(trigger) = &ability.trigger else {
        return Err(unsupported("missing fanfare trigger"));
    };
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &trigger.on
    else {
        return Err(unsupported("fanfare trigger pattern"));
    };
    let expected = Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event: *binding })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }));
    if !matches!(&ability.source.origin, AbilityOrigin::Keyword { keyword: KeywordId::Fanfare, origin } if matches!(origin.as_ref(), AbilityOrigin::Printed { .. }))
        || ability.play.is_some()
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.limit.is_some()
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || *predicate != expected
    {
        return Err(unsupported("printed per-fact fanfare header"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Fanfare has its own authority producer and continuation."
)]
impl Game {
    pub(super) fn play_internal_follower(
        &mut self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<super::CardEntry> {
        let object = self.object(holder)?.clone();
        let seat = seat_name(actor)?;
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        };
        self.check_card_play_holder(holder, actor)?;
        if self.object_type(holder)? != "フォロワー" {
            return Err(unsupported("fanfare card play holder"));
        }
        let card = self
            .card_runtime
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("card is not loaded"))?;
        let fanfares = card
            .card()
            .abilities
            .iter()
            .filter(|ability| ability.node.kind == AbilityKind::Triggered)
            .cloned()
            .collect::<Vec<_>>();
        let [ability] = fanfares.as_slice() else {
            return Err(unsupported("card play requires one fanfare"));
        };
        let (face, _) = printed_origin(&ability.node.source.origin)?;
        if face_ordinal(face)? != 0 {
            return Err(unsupported("fanfare belongs to another face"));
        }
        let ability = ability.clone();
        let mut execution = self.new_card_execution(context)?;
        let decision = json!({"do":"play","card":holder,"by":seat,"at":"main"});
        let mut frame = self.start_frame(holder, serde_json::Value::Null, &decision)?;
        self.internal_play_move(holder, "resolution", &frame)?;
        self.object_mut(holder)?.state["entered_from"] = json!(object.zone);
        let cost = self.internal_effective_cost(holder)?;
        let quantity = self.internal_pay_pp(actor, Int(cost))?;
        if self.zone_count(seat, "field") >= 5 {
            return Err(crate::EngineFailure::IllegalPlay(
                "full field after payment".into(),
            ));
        }
        let played = execution.reference(self, holder)?;
        execution.receipts.insert(
            ReceiptId(0),
            super::InstructionOutcome::Payment {
                quantity,
                subjects: Vec::new(),
            },
        );
        execution.facts.push(super::Fact::CardPlayed {
            execution: execution.id,
            object: played,
            controller: actor,
        });
        self.bump(&format!("{seat}.cards_played"), 1);
        let group = self.group();
        let event = self.emit(
            json!({"kind":"プレイ","object":holder}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":event});
        self.internal_play_move(holder, "field", &frame)?;
        let source = self.internal_card_source(holder, ability.node.id, actor)?;
        self.card_runtime.pending = Some(PendingFanfare {
            holder: holder.into(),
            source,
            ability,
        });
        self.internal_numeric_end_resolution(execution.id)?;
        let resolve_group = self.group();
        self.emit(
            json!({"kind":"解決","object":holder}),
            &frame.cause,
            resolve_group,
        );
        execution.facts.push(super::Fact::CardResolved {
            execution: execution.id,
            object: played,
            controller: actor,
        });
        self.checks()?;
        if self.object(holder)?.generation != source.holder.generation.0
            || self.object(holder)?.zone != "field"
        {
            return Err(unsupported(
                "fanfare source requires last-known information",
            ));
        }
        Ok(super::CardEntry {
            execution: execution.id,
            progress: execution.progress(),
            step: Step {
                outcome: "resolved".into(),
                events: take(&mut self.emitted),
            },
            resume: super::ResumeStage::PendingFanfare,
        })
    }

    /// Resolve only the fanfare occurrence produced by the preceding card play.
    ///
    /// # Errors
    /// Rejects missing occurrences and restores the pending producer after any failure.
    pub fn resolve_internal_fanfare(&mut self) -> Result<CardPlay> {
        let before = self.clone();
        let result = (|| {
            let pending = self
                .card_runtime
                .pending
                .take()
                .ok_or_else(|| invalid("no pending internal fanfare"))?;
            let context = Context {
                source: pending.holder,
                generation: pending.source.holder.generation,
                controller: pending.source.controller,
            };
            self.internal_context(&context)?;
            self.start_internal_ability(pending.ability.node, context, pending.source, false)
        })();
        self.finish_card_attempt(before, result)
    }

    pub(super) fn internal_card_source(
        &mut self,
        holder: &str,
        definition: AbilityId,
        controller: PlayerId,
    ) -> Result<RuntimeAbilitySource> {
        let generation = self.object(holder)?.generation;
        let id = self
            .internal_identities()?
            .into_iter()
            .find(|(_, physical)| physical == holder)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("missing holder identity"))?;
        let key = (holder.into(), generation, definition);
        let instance = if let Some(&instance) = self.card_runtime.instances.get(&key) {
            instance
        } else {
            let next = self
                .card_runtime
                .next_instance
                .checked_add(1)
                .ok_or_else(|| invalid("ability instances exhausted"))?;
            let instance = AbilityInstanceId(next);
            self.card_runtime.next_instance = next;
            self.card_runtime.instances.insert(key, instance);
            instance
        };
        Ok(RuntimeAbilitySource {
            holder: ObjectRef {
                id,
                generation: Generation(generation),
            },
            instance,
            controller,
            definition,
        })
    }
}
