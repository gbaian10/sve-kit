use super::super::{Step, list};
use super::{
    Context, Execution, Fact, Game, InstructionOutcome, Progress, admission, int, player_id,
    seat_name, unsupported,
};
use crate::ir::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, AbilitySource, Action,
    Control, CostSpec, DamageAmount, ExecutionId, Expr, FaceId, Generation, InstructionContext,
    Int, KeywordId, Located, Nat, Node, ObjectOperand, ObjectRef, PlayerId, PlayerRef, Posture,
    PostureEvent, Quantity, QuantityUnit, Resource, ResourceOperation, RuntimeAbilitySource,
    Selector, SourceSpan, StepwiseMode, WellFormedCardProgram, ZoneKind,
};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use core::{mem::take, result, slice};
use serde::Serializer;
use serde::ser::Error as _;
use serde_json::json;

mod fanfare;
pub(super) mod play;
pub(super) mod selection;

pub use play::{CardEntry, CardPlay};
pub use selection::TargetDomain;

/// Explicit authority boundary after a complete ability resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeStage {
    /// Resolve the single authority-produced fanfare before another player action.
    PendingFanfare,
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
    pending: Option<fanfare::PendingFanfare>,
}

impl CardRuntime {
    pub(in crate::game) const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::game) fn requires_typed_play(&self) -> bool {
        self.cards.values().any(|card| {
            card.card().abilities.iter().any(|ability| {
                matches!(
                    ability.node.kind,
                    AbilityKind::Spell | AbilityKind::Triggered
                )
            })
        })
    }
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
    /// Load one complete internal card against this game's exact catalogue.
    /// Candidate base faces map to ordinal zero without granting public qualification.
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
            || self.card_runtime.has_pending()
        {
            return Err(invalid("duplicate card or busy internal card loader"));
        }
        if !list(&self.catalog.program(card_no)?["abilities"]).is_empty() {
            return Err(unsupported("typed card mixed with prototype abilities"));
        }
        let mut origins = BTreeSet::new();
        let mut fanfare_faces = BTreeSet::new();
        let mut spell_face = None;
        for located in &card.card().abilities {
            let ability = &located.node;
            check_header(ability)?;
            let (face, line) = printed_origin(&ability.source.origin)?;
            let ordinal = face_ordinal(face)?;
            if ability.kind == AbilityKind::Spell
                && (ordinal != 0 || spell_face.replace(ordinal).is_some())
            {
                return Err(unsupported("one base Printed Spell per card"));
            }
            if ability.kind == AbilityKind::Triggered && !fanfare_faces.insert(ordinal) {
                return Err(unsupported("multiple fanfares need pending order"));
            }
            if line.0 == 0 || !origins.insert((ordinal, line.0)) {
                return Err(invalid("invalid or duplicate printed ability coordinate"));
            }
            let printed = self.catalog.face(card_no, ordinal)?;
            if ability.kind == AbilityKind::Spell {
                play::check_spell(ability, printed)?;
            }
            check_source(&located.source, card_no, &face.0, line.0)?;
            if matches!(ability.kind, AbilityKind::Activated | AbilityKind::Spell) {
                check_costs(ability)?;
            }
            let body = ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing body"))?;
            check_nodes(&body.steps, &located.source, &ability.source)?;
            admission::check_card(body, &located.source, &ability.source)?;
            play::check(ability, &located.source)?;
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
    ) -> Result<CardPlay> {
        let before = self.clone();
        let result = self.activate_loaded_card(holder, definition, actor);
        self.finish_card_attempt(before, result)
    }

    /// Inspect the authority's explicit next stage without advancing the turn.
    #[must_use]
    pub fn internal_card_stage(&self) -> Option<ResumeStage> {
        if self.card_runtime.has_pending() {
            Some(ResumeStage::PendingFanfare)
        } else {
            self.card_runtime
                .checkpoint
                .map(|_| ResumeStage::CheckTiming)
        }
    }

    /// Finish the pending check timing, preserving the active player's main stage.
    ///
    /// # Errors
    /// Rejects missing stages and restores the complete authority on a failed check.
    pub fn resume_internal_card(&mut self) -> Result<Step> {
        if self.card_runtime.checkpoint.is_none() || self.typed.is_some() {
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
    ) -> Result<CardPlay> {
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
        if ability.node.kind != AbilityKind::Activated
            || object.zone != "field"
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
        if face_ordinal(face)? != usize::try_from(int(&object.state["face"])).map_err(invalid)? {
            return Err(invalid("printed ability belongs to another face"));
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: player_id(&object.controller)?,
        };
        self.internal_context(&context)?;
        // Even unselected cards must have an admitted whole tree before they can intervene.
        self.check_internal_cards()?;
        let source = self.internal_card_source(holder, definition, actor)?;
        self.start_internal_ability(ability.node, context, source, false)
    }

    fn check_internal_cards(&self) -> Result<()> {
        for current in self.state.objects.values() {
            if !self.card_runtime.cards.contains_key(&current.card) {
                return Err(unsupported("context contains an unloaded whole card"));
            }
        }
        Ok(())
    }

    pub(super) fn internal_pay_pp(&mut self, actor: PlayerId, amount: Int) -> Result<Quantity> {
        let player = self.player_mut(seat_name(actor)?)?;
        let current = int(&player.pp["current"]);
        if current < amount.0 {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient PP for whole-card payment".into(),
            ));
        }
        let remaining = current
            .checked_sub(amount.0)
            .ok_or_else(|| invalid("PP payment overflow"))?;
        player.pp["current"] = json!(remaining);
        Ok(Quantity {
            amount,
            unit: QuantityUnit::Pp,
        })
    }

    pub(super) fn internal_card_payment(
        &mut self,
        execution: &mut Execution,
        costs: &CostSpec,
        card_cost: bool,
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
                    let amount = if card_cost {
                        Int(self.internal_effective_cost(&execution.context.source)?)
                    } else {
                        *amount
                    };
                    (
                        self.internal_pay_pp(execution.context.controller, amount)?,
                        Vec::new(),
                    )
                }
                Action::Posture { .. } => {
                    let holder = execution.context.source.clone();
                    if self.object(&holder)?.state["acted"] != false {
                        return Err(crate::EngineFailure::IllegalPlay(
                            "whole-card posture cost requires a standing source".into(),
                        ));
                    }
                    let object = execution.object(self, &ObjectOperand::SourceHolder {})?;
                    let changed = self.change_posture(
                        slice::from_ref(&holder),
                        true,
                        &execution.frame().cause,
                    )?;
                    if changed != [holder] {
                        return Err(crate::EngineFailure::IllegalPlay(
                            "incomplete posture payment".into(),
                        ));
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
    let zones = if ability.kind == AbilityKind::Spell {
        ZoneKind::Resolution
    } else {
        ZoneKind::Field
    };
    if ability.active_zones != [zones]
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || !ability.event_captures.is_empty()
        || !ability.declarations.is_empty()
    {
        return Err(unsupported("whole-card ability header"));
    }
    if ability.kind == AbilityKind::Triggered {
        return fanfare::check_header(ability);
    }
    if !matches!(ability.kind, AbilityKind::Activated | AbilityKind::Spell)
        || ability.trigger.is_some()
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
    {
        return Err(unsupported("whole-card ability kind or origin"));
    }
    if ability.play.is_none() {
        return Err(unsupported("missing whole-card PlaySpec"));
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
        || !((source.owner == "card" && source.field == "text")
            || (face == "base"
                && source.field == "effect"
                && source.fragment.is_some()
                && serde_json::from_str::<serde_json::Value>(&source.owner).is_ok_and(|owner| {
                    owner["kind"] == "face_revision" && owner["revision_id"].is_string()
                })))
    {
        return Err(invalid("card source mapping mismatch"));
    }
    Ok(())
}

fn printed_origin(origin: &AbilityOrigin) -> Result<(&FaceId, &Nat)> {
    match origin {
        AbilityOrigin::Printed { face, line } => Ok((face, line)),
        AbilityOrigin::Keyword {
            keyword: KeywordId::Fanfare,
            origin,
        } => {
            if let AbilityOrigin::Printed { face, line } = origin.as_ref() {
                Ok((face, line))
            } else {
                Err(unsupported("nested fanfare origin"))
            }
        }
        _ => Err(unsupported("non-printed card ability")),
    }
}

fn face_ordinal(face: &FaceId) -> Result<usize> {
    if face.0 == "base" {
        return Ok(0);
    }
    let ordinal = face.0.parse::<usize>().map_err(invalid)?;
    if face.0 != ordinal.to_string() {
        return Err(invalid("invalid printed face ordinal"));
    }
    Ok(ordinal)
}

fn check_nodes(
    nodes: &[Located<Node>],
    source: &SourceSpan,
    ability_source: &AbilitySource,
) -> Result<()> {
    for located in nodes {
        if located.source.owner != source.owner
            || located.source.field != source.field
            || located.source.fragment != source.fragment
        {
            return Err(invalid("instruction source evidence mapping mismatch"));
        }
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
                        amount: DamageAmount::Uniform { .. } | DamageAmount::Allocation { .. },
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
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    check_nodes(&mode.body.steps, source, ability_source)?;
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
