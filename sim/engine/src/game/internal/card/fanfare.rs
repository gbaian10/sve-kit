use super::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, Activation, Context,
    ExecutionId, Expr, Game, Generation, Located, ObjectRef, PlayerId, Result, ResumeStage,
    RuntimeAbilitySource, Step, WellFormedProgram, ZoneKind, admission, face_ordinal, int, invalid,
    json, printed_origin, seat_name, take, unsupported,
};
use crate::ir::{
    EventPattern, EventSnapshotPolicy, KeywordId, ObjectFrozen, Predicate, TriggerAggregation,
    TriggerCondition,
};

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
    /// Play a loaded follower from hand or EX, stopping before its single fanfare.
    /// Candidate `base` maps to catalogue face zero; this grants no public source qualification.
    ///
    /// # Errors
    /// Rejects unsupported cards, timing, costs and interventions atomically.
    pub fn play_internal_card(&mut self, holder: &str, actor: PlayerId) -> Result<Step> {
        let before = self.clone();
        let result = self.play_internal_follower(holder, actor);
        if result.is_err() {
            *self = before;
        }
        result
    }

    fn play_internal_follower(&mut self, holder: &str, actor: PlayerId) -> Result<Step> {
        let object = self.object(holder)?.clone();
        let seat = seat_name(actor)?;
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        };
        self.internal_context(&context)?;
        let point = self.input_point();
        if object.controller != seat
            || object.owner != seat
            || !matches!(object.zone.as_str(), "hand" | "ex")
            || point["at"] != "main"
            || point["by"] != seat
            || self.state.flow["kind"] != "main"
            || self.state.turn["phase"] != "main"
            || self.object_type(holder)? != "フォロワー"
            || int(&object.state["face"]) != 0
            || self.zone_count(seat, "field") >= 5
            || !self.prepared.is_empty()
        {
            return Err(unsupported("fanfare card play holder or timing"));
        }
        self.check_internal_cards()?;
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
        let cost = self.internal_effective_cost(holder)?;
        let pp = self.player(seat)?.pp["current"]
            .as_i64()
            .ok_or_else(|| invalid("invalid current PP"))?;
        if cost < 0 || pp < cost {
            return Err(invalid("unpayable printed play cost"));
        }
        // The prototype movement primitive saturates; prove both new generations first.
        object
            .generation
            .checked_add(2)
            .ok_or_else(|| invalid("play generation exhausted"))?;
        self.emitted.clear();
        self.player_mut(seat)?.pp["current"] = json!(
            pp.checked_sub(cost)
                .ok_or_else(|| invalid("PP payment overflow"))?
        );
        let decision = json!({"do":"play","card":holder,"by":seat,"at":"main"});
        let mut frame = self.start_frame(holder, serde_json::Value::Null, &decision)?;
        self.move_objects(&[holder.into()], "resolution", None, None, &frame)?;
        self.object_mut(holder)?.state["entered_from"] = json!(object.zone);
        self.bump(&format!("{seat}.cards_played"), 1);
        let group = self.group();
        let event = self.emit(
            json!({"kind":"プレイ","object":holder}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":event});
        self.move_objects(&[holder.into()], "field", None, None, &frame)?;
        let source = self.internal_card_source(holder, ability.node.id, actor)?;
        self.card_runtime.pending = Some(PendingFanfare {
            holder: holder.into(),
            source,
            ability,
        });
        let resolve_group = self.group();
        self.emit(
            json!({"kind":"解決","object":holder}),
            &frame.cause,
            resolve_group,
        );
        self.checks()?;
        if self.object(holder)?.generation != source.holder.generation.0
            || self.object(holder)?.zone != "field"
        {
            return Err(unsupported(
                "fanfare source requires last-known information",
            ));
        }
        Ok(Step {
            outcome: "resolved".into(),
            events: take(&mut self.emitted),
        })
    }

    /// Resolve only the fanfare occurrence produced by the preceding card play.
    ///
    /// # Errors
    /// Rejects missing occurrences and restores the pending producer after any failure.
    pub fn resolve_internal_fanfare(&mut self) -> Result<Activation> {
        let before = self.clone();
        let result = (|| {
            let pending = self
                .card_runtime
                .pending
                .take()
                .ok_or_else(|| invalid("no pending internal fanfare"))?;
            let body = pending
                .ability
                .node
                .body
                .ok_or_else(|| invalid("missing fanfare body"))?;
            let program = WellFormedProgram::check(body).map_err(invalid)?;
            admission::check_card(&program, &pending.ability.source)?;
            let context = Context {
                source: pending.holder,
                generation: pending.source.holder.generation,
                controller: pending.source.controller,
            };
            let play = self.prepare_admitted_internal_play(&program, context)?;
            let execution = ExecutionId(play.execution);
            let progress = self.resolve_internal_play(&play, Some(pending.source), None)?;
            self.card_runtime.checkpoint = Some(execution);
            Ok(Activation {
                execution,
                source: pending.source,
                progress,
                resume: ResumeStage::CheckTiming,
            })
        })();
        if result.is_err() {
            *self = before;
        }
        result
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
