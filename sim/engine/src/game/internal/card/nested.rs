use super::{
    Ability, AbilityKind, Context, Execution, Game, Generation, PlayerId, Result, invalid,
    printed_origin, seat_name, unsupported,
};
use crate::game::int;
use crate::game::internal::{InstructionOutcome, player_id};
use crate::ir::nested_play;
use crate::ir::{
    Action, CostOverride, Instruction, ObjectOperand, ReceiptId, RuntimeAbilitySource, Selector,
    ZoneKind,
};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct Continuation {
    pub(in crate::game::internal) parent: Execution,
    before: Box<Game>,
    receipt: ReceiptId,
    cost_override: Option<CostOverride>,
    pub(in crate::game::internal) committed: bool,
}

pub(in crate::game::internal) fn check(action: &Action) -> Result<()> {
    if matches!(action, Action::PlayCard { .. }) && nested_play::card_operand(action).is_none() {
        return Err(unsupported("effect card play shape"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Nested plays share the ordinary formal play authority and its continuation."
)]
impl Game {
    pub(super) fn prepare_card_execution(
        &mut self,
        holder: &str,
        actor: PlayerId,
        nested: Option<Box<Continuation>>,
    ) -> Result<Execution> {
        self.require_internal_abilities(holder, "card play")?;
        let object = self.object(holder)?.clone();
        if nested.is_none() {
            self.check_card_play_holder(holder, actor)?;
        } else {
            if object.zone != "cemetery"
                || int(&object.state["face"]) != 0
                || object.state["evolved_with"].is_string()
                || self.restricted(holder, "play")?
                || self.internal_card_play_forbidden(holder, actor)?
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "effect card play is no longer legal".into(),
                ));
            }
            self.check_internal_resources()?;
        }
        let ability = self.card_play_ability(holder)?;
        let spell = ability.is_some();
        if !spell && self.object_type(holder)?.contains("トークン") && object.zone != "ex" {
            return Err(unsupported("field token play origin"));
        }
        if nested.is_none()
            && let Some(ability) = ability
        {
            let mut execution = self.spell_execution(holder, actor, &ability)?;
            self.prepare_ability_play(&mut execution, ability)?;
            return Ok(execution);
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        };
        let mut execution = if nested.is_some() {
            self.fresh_card_execution(context)?
        } else {
            self.new_card_execution(context)?
        };
        execution.play_origin = Some(match object.zone.as_str() {
            "hand" => ZoneKind::Hand,
            "ex" => ZoneKind::Ex,
            "cemetery" => ZoneKind::Cemetery,
            _ => return Err(unsupported("card play origin")),
        });
        execution.play_origin_player = Some(player_id(&object.controller)?);
        execution.cost_override = nested
            .as_ref()
            .and_then(|entry| entry.cost_override.clone());
        execution.nested = nested;
        if let Some(ability) = &ability {
            execution.spell = true;
            execution.captures(
                self,
                &ability
                    .play
                    .as_ref()
                    .ok_or_else(|| invalid("missing Spell play"))?
                    .captures,
            )?;
        }
        if execution.nested.is_some() {
            self.internal_rule_carry_ready(holder)?;
            let departing_source = execution.source_before_move(self, &[holder.into()])?;
            self.move_effect_playing_card(
                holder,
                seat_name(actor)?,
                "resolution",
                &execution.frame(),
            )?;
            execution.seal_source_departure(&departing_source);
        } else {
            self.internal_play_move(holder, "resolution", &execution.frame())?;
        }
        self.object_mut(holder)?.state["entered_from"] = serde_json::json!(object.zone);
        execution.context.generation = Generation(self.object(holder)?.generation);
        if let Some(ability) = ability {
            let source = self.internal_card_source(holder, ability.id, actor)?;
            execution.bind_source(self, source)?;
            self.prepare_ability_play(&mut execution, ability)?;
        } else {
            self.prepare_field_play(&mut execution)?;
        }
        Ok(execution)
    }

    fn card_play_ability(&self, holder: &str) -> Result<Option<Ability>> {
        match self.object_type(holder)? {
            "フォロワー" | "アミュレット" | "フォロワー・トークン" | "アミュレット・トークン" =>
            {
                return Ok(None);
            }
            "スペル" | "スペル・トークン" | "スペル・アドバンス" => {}
            _ => {
                return Err(crate::EngineFailure::IllegalPlay(
                    "effect cannot play this card face".into(),
                ));
            }
        }
        let object = self.object(holder)?;
        let definition = self
            .card_runtime
            .definitions
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("card is not loaded"))?;
        let abilities = definition
            .card()
            .abilities
            .iter()
            .filter(|a| a.node.kind == AbilityKind::Spell)
            .collect::<Vec<_>>();
        let [located] = abilities.as_slice() else {
            return Err(unsupported("one printed Spell per card"));
        };
        let ability = located.node.clone();
        let (face, _) = printed_origin(&ability.source.origin)?;
        if self.printed_face_ordinal(&object.card, face)? != 0 {
            return Err(unsupported("Spell belongs to another face"));
        }
        Ok(Some(ability))
    }

    pub(in crate::game::internal) fn internal_nested_card(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check(&instruction.action)?;
        let Action::PlayCard {
            card: ObjectOperand::Selected { selection },
            cost_override,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("effect card play"));
        };
        let selected = execution.objects(
            self,
            &Selector::Bound {
                reference: *selection,
            },
        )?;
        let [card] = selected.as_slice() else {
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::NestedPlay(None));
            return Ok(());
        };
        let holder = execution
            .ids
            .get(&card.id)
            .ok_or_else(|| invalid("missing selected card identity"))?
            .clone();
        if self.object(&holder)?.generation != card.generation.0
            || self.object(&holder)?.zone != "cemetery"
        {
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::NestedPlay(None));
            return Ok(());
        }
        let before = self.clone();
        let continuation = Box::new(Continuation {
            parent: execution.clone(),
            before: Box::new(before.clone()),
            receipt: instruction.receipt,
            cost_override: cost_override.clone(),
            committed: false,
        });
        match self.prepare_card_execution(&holder, execution.context.controller, Some(continuation))
        {
            Ok(child) => *execution = child,
            Err(crate::EngineFailure::IllegalPlay(_)) => {
                let epoch = self.state.ir_epoch;
                *self = before;
                self.state.ir_epoch = self.state.ir_epoch.max(epoch);
                execution
                    .receipts
                    .insert(instruction.receipt, InstructionOutcome::NestedPlay(None));
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    pub(in crate::game::internal) fn finish_nested_card(execution: &mut Execution) -> bool {
        let Some(nested) = execution.nested.take() else {
            return false;
        };
        let mut parent = nested.parent;
        parent.facts.append(&mut execution.facts);
        parent.receipts.insert(
            nested.receipt,
            InstructionOutcome::NestedPlay(Some(execution.id)),
        );
        *execution = parent;
        true
    }

    pub(in crate::game::internal) fn cancel_nested_card(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        let nested = execution
            .nested
            .take()
            .ok_or_else(|| invalid("missing child play transaction"))?;
        let epoch = self.state.ir_epoch;
        // Dispatch owns the live source; its temporary binding is outside child rollback.
        let active_source = self.card_runtime.active_source;
        *self = *nested.before;
        self.state.ir_epoch = self.state.ir_epoch.max(epoch);
        self.card_runtime.active_source = active_source;
        let mut parent = nested.parent;
        parent.restore_source_liveness(self);
        parent
            .receipts
            .insert(nested.receipt, InstructionOutcome::NestedPlay(None));
        *execution = parent;
        Ok(())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Only the private continuation module can inspect parent execution ownership."
)]
impl Execution {
    pub(in crate::game::internal) fn root_ability(&self) -> Option<RuntimeAbilitySource> {
        self.nested
            .as_ref()
            .map_or(self.ability, |nested| nested.parent.root_ability())
    }
}
