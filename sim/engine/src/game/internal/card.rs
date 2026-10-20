use super::super::{Step, list};
use super::{
    Context, Execution, Fact, Game, InstructionOutcome, Progress, admission, int, player_id,
    seat_name, unsupported,
};
use crate::ir::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, AbilitySource, Action,
    Control, CostSpec, DamageAmount, ExecutionId, Expr, Generation, InstructionContext, Int,
    Located, Node, ObjectOperand, ObjectRef, PlayerId, PlayerRef, Posture, PostureEvent, Quantity,
    QuantityUnit, Resource, ResourceOperation, RuntimeAbilitySource, Selector, SourceSpan,
    StepwiseMode, WellFormedCardProgram, WellFormedProgram, ZoneKind,
};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{mem::take, result, slice};
use serde::Serializer;
use serde::ser::Error as _;
use serde_json::json;

/// Explicit authority boundary after a complete ability resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeStage {
    /// Run state-based checks before accepting another player action.
    CheckTiming,
}

/// Committed authority facts; no player or wire serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    /// Authority-allocated producer for this attempt.
    pub execution: ExecutionId,
    /// Instance and source fixed at activation.
    pub source: RuntimeAbilitySource,
    /// Complete body receipts and ordered play/body/resolve facts.
    pub progress: Progress,
    /// The next rules stage, also retained by the game.
    pub resume: ResumeStage,
}

#[derive(Clone, Debug, Default)]
pub(in crate::game) struct CardRuntime {
    cards: BTreeMap<String, WellFormedCardProgram>,
    instances: BTreeMap<(String, u64, AbilityId), AbilityInstanceId>,
    next_instance: u64,
    pub(in crate::game) checkpoint: Option<ExecutionId>,
}

impl CardRuntime {
    pub(in crate::game) fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

pub(in crate::game) fn serialize_cards<S>(_: &CardRuntime, _: S) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    Err(S::Error::custom(
        "loaded internal cards have no save format",
    ))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Whole-card admission and activation share the authoritative game."
)]
impl Game {
    /// Load one complete hand-written card against this game's exact catalogue.
    /// This internal boundary uses card numbers and numeric face ordinals only.
    ///
    /// # Errors
    /// Rejects unsupported abilities, play parameters or captures, foreign provenance,
    /// prototype abilities and duplicate registrations without changing the game.
    pub fn load_internal_card(
        &mut self,
        card_no: &str,
        card: &WellFormedCardProgram,
    ) -> Result<()> {
        self.catalog.face(card_no, 0)?;
        if self.card_runtime.cards.contains_key(card_no)
            || self.typed.is_some()
            || !self.prepared.is_empty()
            || self.card_runtime.checkpoint.is_some()
        {
            return Err(invalid("duplicate card or busy internal card loader"));
        }
        if !list(&self.catalog.program(card_no)?["abilities"]).is_empty() {
            return Err(unsupported("typed card mixed with prototype abilities"));
        }
        let mut origins = BTreeSet::new();
        for located in &card.card().abilities {
            let ability = &located.node;
            check_header(ability)?;
            let AbilityOrigin::Printed { face, line } = &ability.source.origin else {
                return Err(unsupported("non-printed card ability"));
            };
            let ordinal = face.0.parse::<usize>().map_err(invalid)?;
            if face.0 != ordinal.to_string() || line.0 == 0 || !origins.insert((ordinal, line.0)) {
                return Err(invalid("invalid or duplicate printed ability coordinate"));
            }
            self.catalog.face(card_no, ordinal)?;
            check_source(&located.source, card_no, &face.0, line.0)?;
            check_costs(ability)?;
            let body = ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing body"))?;
            check_nodes(&body.steps, &located.source, &ability.source)?;
            let program = WellFormedProgram::check(body.clone()).map_err(invalid)?;
            admission::check(&program, &located.source)?;
        }
        self.card_runtime.cards.insert(card_no.into(), card.clone());
        Ok(())
    }

    /// Activate a loaded, printed, field ability at the active player's main timing.
    /// Holder generation, origin, instance and execution are derived by authority.
    ///
    /// # Errors
    /// Rejects stale timing, foreign definitions, unhandled interventions or execution
    /// failures atomically, including identities, random state, observations and facts.
    pub fn activate_internal_card(
        &mut self,
        holder: &str,
        definition: AbilityId,
        actor: PlayerId,
    ) -> Result<Activation> {
        let before = self.clone();
        let result = self.activate_loaded_card(holder, definition, actor);
        if result.is_err() {
            *self = before;
        }
        result
    }

    /// Inspect the authority's explicit next stage without advancing the turn.
    #[must_use]
    pub fn internal_card_stage(&self) -> Option<ResumeStage> {
        self.card_runtime
            .checkpoint
            .map(|_| ResumeStage::CheckTiming)
    }

    /// Finish the pending check timing, preserving the active player's main stage.
    ///
    /// # Errors
    /// Rejects missing stages and restores the complete authority on a failed check.
    pub fn resume_internal_card(&mut self) -> Result<Step> {
        if self.card_runtime.checkpoint.is_none() {
            return Err(invalid("no internal card check timing"));
        }
        let before = self.clone();
        self.emitted.clear();
        let result = self.checks();
        if result.is_err() {
            *self = before;
        }
        result?;
        self.card_runtime.checkpoint = None;
        Ok(Step {
            outcome: "resolved".into(),
            events: take(&mut self.emitted),
        })
    }

    fn activate_loaded_card(
        &mut self,
        holder: &str,
        definition: AbilityId,
        actor: PlayerId,
    ) -> Result<Activation> {
        let object = self.object(holder)?.clone();
        let ability = self
            .card_runtime
            .cards
            .get(&object.card)
            .and_then(|card| {
                card.card()
                    .abilities
                    .iter()
                    .find(|a| a.node.id == definition)
            })
            .cloned()
            .ok_or_else(|| invalid("ability is not loaded for this holder"))?;
        let seat = seat_name(actor)?;
        let point = self.input_point();
        if object.zone != "field"
            || object.controller != seat
            || point["at"] != "main"
            || point["by"] != seat
            || self.state.flow["kind"] != "main"
            || self.state.turn["phase"] != "main"
            || object.state["silenced"] == true
            || object.state["evolved_with"].is_string()
            || self.object_type(holder)? != "フォロワー"
            || !self.prepared.is_empty()
        {
            return Err(unsupported("activation holder, controller or timing"));
        }
        let AbilityOrigin::Printed { face, .. } = &ability.node.source.origin else {
            return Err(unsupported("activation origin"));
        };
        if face.0 != int(&object.state["face"]).to_string() {
            return Err(invalid("printed ability belongs to another face"));
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: player_id(&object.controller)?,
        };
        self.internal_context(&context)?;
        // Even unselected cards must have an admitted whole tree before they can intervene.
        for current in self.state.objects.values() {
            if !self.card_runtime.cards.contains_key(&current.card) {
                return Err(unsupported("context contains an unloaded whole card"));
            }
        }
        let ids = self.internal_identities()?;
        let id = ids
            .iter()
            .find(|(_, physical)| physical.as_str() == holder)
            .map(|(id, _)| *id)
            .ok_or_else(|| invalid("missing holder identity"))?;
        let key = (holder.into(), object.generation, definition);
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
        let source = RuntimeAbilitySource {
            holder: ObjectRef {
                id,
                generation: context.generation,
            },
            instance,
            controller: actor,
            definition,
        };
        let costs = &ability
            .node
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing play"))?
            .costs;
        let body = ability
            .node
            .body
            .ok_or_else(|| invalid("missing loaded body"))?;
        let program = WellFormedProgram::check(body).map_err(invalid)?;
        let play = self.prepare_internal_play(&program, &ability.source, context)?;
        let execution = ExecutionId(play.execution);
        let progress = self.resolve_internal_play(&play, Some(source), Some(costs))?;
        self.card_runtime.checkpoint = Some(execution);
        Ok(Activation {
            execution,
            source,
            progress,
            resume: ResumeStage::CheckTiming,
        })
    }

    pub(super) fn internal_card_payment(
        &mut self,
        execution: &mut Execution,
        costs: &CostSpec,
    ) -> Result<()> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("whole-card cost alternatives"));
        };
        for step in steps {
            let (quantity, subjects) = match &step.instruction.action {
                Action::Resource {
                    amount: Expr::Literal(amount),
                    ..
                } => {
                    let seat = seat_name(execution.context.controller)?;
                    let player = self.player_mut(seat)?;
                    let current = int(&player.pp["current"]);
                    if current < amount.0 {
                        return Err(invalid("insufficient PP for whole-card payment"));
                    }
                    let remaining = current
                        .checked_sub(amount.0)
                        .ok_or_else(|| invalid("PP payment overflow"))?;
                    player.pp["current"] = json!(remaining);
                    (
                        Quantity {
                            amount: *amount,
                            unit: QuantityUnit::Pp,
                        },
                        Vec::new(),
                    )
                }
                Action::Posture { .. } => {
                    let holder = execution.context.source.clone();
                    if self.object(&holder)?.state["acted"] != false {
                        return Err(invalid(
                            "whole-card posture cost requires a standing source",
                        ));
                    }
                    let object = execution.object(self, &ObjectOperand::SourceHolder {})?;
                    let changed = self.change_posture(
                        slice::from_ref(&holder),
                        true,
                        &execution.frame().cause,
                    )?;
                    if changed != [holder] {
                        return Err(invalid("incomplete posture payment"));
                    }
                    execution.facts.push(Fact::Posture(PostureEvent {
                        object,
                        before: Posture::Standing,
                        after: Posture::Acted,
                    }));
                    (
                        Quantity {
                            amount: Int(1),
                            unit: QuantityUnit::Cards,
                        },
                        vec![object],
                    )
                }
                _ => return Err(unsupported("whole-card cost step")),
            };
            execution.receipts.insert(
                step.instruction.receipt,
                InstructionOutcome::Payment { quantity, subjects },
            );
        }
        Ok(())
    }
}

fn check_header(ability: &Ability) -> Result<()> {
    if ability.kind != AbilityKind::Activated
        || ability.active_zones != [ZoneKind::Field]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !ability.event_captures.is_empty()
        || !ability.declarations.is_empty()
        || ability.trigger.is_some()
    {
        return Err(unsupported("whole-card ability header"));
    }
    let Some(play) = &ability.play else {
        return Err(unsupported("missing whole-card PlaySpec"));
    };
    if !play.captures.is_empty() || !play.parameters.is_empty() {
        return Err(unsupported("whole-card captures or parameters"));
    }
    Ok(())
}

fn check_costs(ability: &Ability) -> Result<()> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing play"))?;
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("whole-card cost alternatives"));
    };
    for step in steps {
        let instruction = &step.instruction;
        if step.materials.is_some()
            || instruction.context != InstructionContext::Cost
            || instruction.precondition.is_some()
            || instruction.source != ability.source
            || instruction.actor != (PlayerRef::Controller {})
        {
            return Err(unsupported("whole-card cost metadata or source"));
        }
        let supported = match &instruction.action {
            Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(amount)),
            } => *amount >= 0,
            Action::Posture {
                objects: Selector::Objects { objects },
                posture: Posture::Acted,
            } => objects == &[ObjectOperand::SourceHolder {}],
            _ => false,
        };
        if !supported {
            return Err(unsupported("whole-card cost step"));
        }
    }
    Ok(())
}

fn check_source(source: &SourceSpan, card: &str, face: &str, line: u64) -> Result<()> {
    if source.card_no != card
        || source.card_id != card
        || source.face != face
        || u64::from(source.line) != line
        || source.owner != "card"
        || source.field != "text"
    {
        return Err(invalid("card source mapping mismatch"));
    }
    Ok(())
}

fn check_nodes(
    nodes: &[Located<Node>],
    source: &SourceSpan,
    ability_source: &AbilitySource,
) -> Result<()> {
    for located in nodes {
        check_source(
            &located.source,
            &source.card_no,
            &source.face,
            u64::from(source.line),
        )?;
        match &located.node {
            Node::Instruction { instruction } => {
                if instruction.source != *ability_source {
                    return Err(invalid("instruction has a foreign ability source"));
                }
                if !matches!(
                    instruction.action,
                    Action::Draw {
                        mode: StepwiseMode::Exact,
                        ..
                    } | Action::Look {
                        mode: StepwiseMode::Exact,
                        ..
                    } | Action::Damage {
                        amount: DamageAmount::Uniform { .. },
                        ..
                    } | Action::Modify { .. }
                        | Action::InstallEffect { .. }
                ) {
                    return Err(unsupported("whole-card synchronous action"));
                }
            }
            Node::Control {
                control: Control::Sequence { body },
            } => {
                check_nodes(&body.steps, source, ability_source)?;
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_nodes(&then_body.steps, source, ability_source)?;
                if let Some(body) = else_body {
                    check_nodes(&body.steps, source, ability_source)?;
                }
            }
            Node::Capture { .. } => {}
            Node::Control { .. } | Node::Decision { .. } => {
                return Err(unsupported("whole-card synchronous node"));
            }
        }
    }
    Ok(())
}
