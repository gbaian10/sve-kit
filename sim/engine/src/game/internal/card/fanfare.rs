use super::{
    Ability, AbilityId, AbilityInstanceId, AbilityOrigin, Expr, Game, Generation, ObjectRef,
    PlayerId, Result, RuntimeAbilitySource, Step, ZoneKind, invalid, json, seat_name, take,
    unsupported,
};
use crate::ir::LimitSpec;
use crate::ir::optional_payment;
use crate::ir::{
    EventPattern, EventSnapshotPolicy, KeywordId, ObjectFrozen, Predicate, ReceiptId,
    TriggerAggregation, TriggerCondition,
};

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
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
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
    pub(super) fn play_internal_field_card(
        &mut self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<super::CardPlay> {
        let mut execution = self.prepare_card_execution(holder, actor, None)?;
        self.internal_run(&mut execution)?;
        self.card_play_result(execution)
    }

    pub(in crate::game::internal) fn internal_field_card_finish(
        &mut self,
        execution: &mut super::Execution,
    ) -> Result<()> {
        let holder = execution.context.source.clone();
        let actor = execution.context.controller;
        let seat = seat_name(actor)?;
        let mut decision = json!({"do":"play","card":holder,"by":seat});
        if execution.nested.is_none() {
            decision["at"] = json!("main");
        }
        let mut frame = self.start_frame(&holder, serde_json::Value::Null, &decision)?;
        if self.zone_count(seat, "field") >= 5 {
            return Err(crate::EngineFailure::IllegalPlay(
                "full field after payment".into(),
            ));
        }
        if self.internal_ward_entry_choice(execution, &holder)? {
            return Ok(());
        }
        let played = execution.reference(self, &holder)?;
        if let Some(nested) = execution.nested.as_mut() {
            nested.committed = true;
        }
        execution.facts.push(super::Fact::CardPlayed {
            execution: execution.id,
            object: played,
            controller: actor,
        });
        self.bump(&format!("{seat}.cards_played"), 1);
        self.internal_played_card_events(execution, ReceiptId(0))?;
        let group = self.group();
        let event = self.emit(
            json!({"kind":"プレイ","object":holder}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":event});
        if execution.nested.is_some() {
            self.move_effect_playing_card(&holder, seat, "field", &frame)?;
        } else {
            self.internal_play_move(&holder, "field", &frame)?;
        }
        self.internal_ward_entry_posture(execution, played)?;
        self.internal_numeric_end_resolution(execution.id)?;
        self.internal_preventions_end_resolution(execution.id);
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
        if execution.nested.is_some() {
            return Ok(());
        }
        self.checks()?;
        self.card_runtime.entry_result = Some(super::CardEntry {
            execution: execution.id,
            progress: execution.progress(),
            step: Step {
                outcome: "resolved".into(),
                events: take(&mut self.emitted),
            },
            resume: if self.card_runtime.has_pending() {
                super::ResumeStage::Pending
            } else {
                self.card_runtime.checkpoint = Some(execution.id);
                super::ResumeStage::CheckTiming
            },
        });
        Ok(())
    }

    pub(in crate::game::internal) fn internal_card_source(
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
        let (printed, face) = self.internal_effective_print(holder)?;
        let key = (holder.into(), generation, printed, face, definition);
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
