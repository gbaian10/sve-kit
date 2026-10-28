use super::super::{Step, list};
use super::{
    Context, Execution, Fact, Game, InstructionOutcome, Progress, admission, int, player_id,
    seat_name, unsupported,
};
use crate::catalog::SelectedEffects;
use crate::catalog::current::CurrentCard;
use crate::ir::ChoiceDomain;
use crate::ir::quick::{card_permission, is_printed_quick};
use crate::ir::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, AbilitySource, Action,
    AssociationId, Control, CostSpec, DamageAmount, ExecutionId, Expr, FaceId, Generation,
    InstructionContext, Int, KeywordId, Located, MoveDestination, MoveReason, Nat, Node,
    NumericChange, ObjectOperand, ObjectRef, OwnerZone, PlayerId, PlayerRef, Posture, PostureEvent,
    Quantity, QuantityUnit, Resource, ResourceOperation, RuleId, RuntimeAbilitySource, Selector,
    SourceSpan, StepwiseMode, WellFormedCardProgram, ZoneKind,
};
use crate::ir::{ConsumptionPoint, LimitSpec, ModeUse};
use crate::ir::{TurnId, printed_keywords};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::{mem::take, result, slice};
use serde::Serializer;
use serde::ser::Error as _;
use serde_json::json;

mod costs;
mod evolution;
pub(super) mod exchange;
mod fanfare;
mod keywords;
pub(super) mod optional;
mod printed;
pub(super) mod quick;
mod resources;
pub(in crate::game::internal) mod triggers;
pub use triggers::{PendingOption, QuotaDomain, QuotaSelection};
pub(super) mod play;
pub(super) mod recipient_targets;
pub(super) mod selection;

pub use play::{CardEntry, CardPlay};
pub use selection::TargetDomain;

/// Explicit authority boundary after a complete ability resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeStage {
    /// Choose and resolve authority-produced pending occurrences before another player action.
    Pending,
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
    selected: bool,
    cards: BTreeMap<String, WellFormedCardProgram>,
    current: BTreeMap<String, Arc<CurrentCard>>,
    resources: BTreeMap<String, String>,
    unadmitted: BTreeSet<String>,
    instances: BTreeMap<(String, u64, String, usize, AbilityId), AbilityInstanceId>,
    links: BTreeMap<(u64, u64, u64, u64), AssociationId>,
    quotas: BTreeMap<(AbilityInstanceId, ConsumptionPoint), (TurnId, u64)>,
    mode_uses: Vec<ModeUse>,
    admission: Option<triggers::Admission>,
    next_instance: u64,
    entry_result: Option<CardEntry>,
    pub(in crate::game) checkpoint: Option<ExecutionId>,
    pending: Vec<triggers::PendingOccurrence>,
    pub(in crate::game::internal) question: Option<(super::DecisionView, Vec<u64>)>,
    next_pending: u64,
    rule_ids: BTreeMap<String, RuleId>,
    pub(in crate::game::internal) active_source: Option<RuntimeAbilitySource>,
    pub(in crate::game::internal) quick: Option<quick::Question>,
    pub(in crate::game::internal) quick_start: bool,
}

impl CardRuntime {
    fn quota_available(&self, instance: AbilityInstanceId, turn: TurnId, limit: LimitSpec) -> bool {
        self.quotas
            .get(&(instance, limit.consume_at))
            .is_none_or(|(used_turn, used)| *used_turn != turn || *used < limit.maximum.0)
    }

    fn consume_quota(
        &mut self,
        instance: AbilityInstanceId,
        turn: TurnId,
        limit: LimitSpec,
    ) -> Result<()> {
        if !self.quota_available(instance, turn, limit) {
            return Err(crate::EngineFailure::IllegalPlay(
                "ability quota exhausted".into(),
            ));
        }
        let entry = self
            .quotas
            .entry((instance, limit.consume_at))
            .or_insert((turn, 0));
        if entry.0 != turn {
            *entry = (turn, 0);
        }
        entry.1 = entry
            .1
            .checked_add(1)
            .ok_or_else(|| invalid("ability quota overflow"))?;
        Ok(())
    }

    pub(in crate::game::internal) fn terminate(&mut self) {
        self.pending.clear();
        self.unadmitted.clear();
        self.admission = None;
        self.question = None;
        self.checkpoint = None;
        self.active_source = None;
        self.quick = None;
        self.quick_start = false;
    }

    pub(in crate::game) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.unadmitted.is_empty()
    }

    pub(in crate::game) fn requires_typed_play(&self) -> bool {
        self.cards.values().any(|card| {
            !card.card().card_play_costs.is_empty()
                || card
                    .card()
                    .abilities
                    .iter()
                    .any(|a| is_printed_quick(&a.node))
                || card.card().abilities.iter().any(|ability| {
                    evolution::intrinsic(&ability.node)
                        || matches!(
                            ability.node.kind,
                            AbilityKind::Spell | AbilityKind::Triggered
                        )
                })
        })
    }
    pub(in crate::game) fn is_empty(&self) -> bool {
        self.cards.is_empty() && !self.selected
    }

    pub(in crate::game) fn is_loaded(&self, card: &str) -> bool {
        self.cards.contains_key(card)
    }

    pub(super) fn has_definition(&self, card: &str) -> bool {
        self.cards.contains_key(card) || self.current.contains_key(card)
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
    /// Construct a game from the complete accepted selection and admit every current card.
    /// Provenance remains authority-only; loaded cards retain the existing save rejection.
    ///
    /// # Errors
    /// Rejects snapshot/source mismatches and any unsupported whole card without returning a game.
    pub fn from_selected_effects(
        selected: SelectedEffects,
        setup: &serde_json::Value,
        facts: &serde_json::Value,
        random: &serde_json::Value,
        seed: &str,
    ) -> Result<Self> {
        selected.check_runtime()?;
        let catalog = selected.prototypes;
        let mut current = BTreeMap::new();
        for (number, candidate) in selected.current {
            if number != candidate.card_no {
                return Err(invalid("current selection card key mismatch"));
            }
            current.insert(number, Arc::new(catalog.resolve_current(candidate)?));
        }
        let mut game = Self::new(catalog, setup, facts, random, seed)?;
        if game.state.objects.values().any(|object| {
            game.catalog
                .rejected
                .get(&object.card)
                .is_some_and(|reasons| {
                    reasons
                        .iter()
                        .any(|reason| reason == "source_unresolved_issue")
                })
        }) {
            return Err(unsupported("a selected card has an unresolved ruling"));
        }
        game.card_runtime.selected = true;
        game.card_runtime.current = current;
        game.admit_evolution_resources()?;
        let cards = game
            .card_runtime
            .current
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for card in cards {
            game.load_internal_card(&card.candidate.card_no, &card.candidate.program)?;
        }
        Ok(game)
    }

    fn printed_face_ordinal(&self, card: &str, face: &FaceId) -> Result<usize> {
        self.card_runtime
            .current
            .get(card)
            .map_or_else(|| face_ordinal(face), |current| current.ordinal(face))
    }

    fn printed_face_id(&self, card: &str, ordinal: usize) -> Result<FaceId> {
        self.card_runtime.current.get(card).map_or_else(
            || Ok(FaceId(ordinal.to_string())),
            |current| current.face(ordinal),
        )
    }

    fn check_card_source(
        &self,
        source: &SourceSpan,
        card: &str,
        face: &str,
        line: u64,
    ) -> Result<()> {
        if let Some(current) = self.card_runtime.current.get(card) {
            if source.face != face || u64::from(source.line) != line {
                return Err(invalid("current printed source coordinate mismatch"));
            }
            return current.check_source(source);
        }
        check_source(source, card, face, line)
    }

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
        let base_print = self.catalog.face(card_no, 0)?;
        check_printed_face(base_print)?;
        if matches!(
            base_print["card_type"].as_str(),
            Some("フォロワー" | "スペル" | "アミュレット")
        ) {
            super::cost::printed_play_cost(&base_print["cost"])?;
        }
        costs::check(self, card_no, card)?;
        if self.card_runtime.cards.contains_key(card_no)
            || self.typed.is_some()
            || !self.prepared.is_empty()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.quick.is_some()
        {
            return Err(invalid("duplicate card or busy internal card loader"));
        }
        match self.card_runtime.current.get(card_no) {
            Some(current) if card != &current.candidate.program => {
                return Err(invalid("card differs from accepted current selection"));
            }
            None if !list(&self.catalog.program(card_no)?["abilities"]).is_empty() => {
                return Err(unsupported("typed card mixed with prototype abilities"));
            }
            _ => {}
        }
        let mut origins = BTreeSet::new();
        let mut spell_face = None;
        for located in &card.card().abilities {
            let ability = &located.node;
            check_header(ability)?;
            let (face, line) = printed_origin(&ability.source.origin)?;
            let ordinal = self.printed_face_ordinal(card_no, face)?;
            if ability.kind == AbilityKind::Spell
                && (ordinal != 0 || spell_face.replace(ordinal).is_some())
            {
                return Err(unsupported("one base Printed Spell per card"));
            }
            let declaration = printed_keywords::card_declaration(ability);
            keywords::check_coordinate(&mut origins, ordinal, line.0, declaration)?;
            let printed = self.catalog.face(card_no, ordinal)?;
            check_printed_face(printed)?;
            if is_printed_quick(ability)
                && ability.kind == AbilityKind::Activated
                && printed["card_type"] != "フォロワー"
            {
                return Err(unsupported(
                    "Quick activation requires an ordinary follower",
                ));
            }
            if ability.kind == AbilityKind::Activated
                && !printed["card_type"].as_str().is_some_and(activation_type)
            {
                return Err(unsupported(
                    "activated printed face is not an ordinary field card",
                ));
            }
            if ability.kind == AbilityKind::Spell {
                play::check_spell(ability, printed)?;
            }
            self.check_card_source(&located.source, card_no, &face.0, line.0)?;
            if let Some(keyword) = declaration {
                keywords::check(self, card_no, located, printed, keyword)?;
                continue;
            }
            if card_permission(ability) {
                quick::check_spell_face(self, card_no, card.card(), ordinal, base_print)?;
                continue;
            }
            if is_printed_quick(ability) && ordinal != 0 {
                return Err(unsupported("Quick special face"));
            }
            if evolution::intrinsic(ability) {
                evolution::check(self, located)?;
                continue;
            }
            if matches!(ability.kind, AbilityKind::Activated | AbilityKind::Spell) {
                check_costs(ability)?;
            }
            let body = ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing body"))?;
            optional::check_body(body, true)?;
            check_nodes(self, &body.steps, &located.source, &ability.source)?;
            admission::check_card(
                body,
                &located.source,
                &ability.source,
                ability.trigger.as_ref(),
            )?;
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
        if self.card_runtime.quick.is_some() {
            return Err(invalid("Quick requires its authority question"));
        }
        let before = self.clone();
        let result = self.activate_loaded_card(holder, definition, actor);
        self.finish_card_attempt(before, result)
    }

    /// Inspect the authority's explicit next stage without advancing the turn.
    #[must_use]
    pub fn internal_card_stage(&self) -> Option<ResumeStage> {
        if self.card_runtime.checkpoint.is_some() {
            Some(ResumeStage::CheckTiming)
        } else if self.card_runtime.has_pending() {
            Some(ResumeStage::Pending)
        } else {
            None
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
        if self.state.game["ended"] == true {
            self.card_runtime.pending.clear();
            self.card_runtime.question = None;
        }
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
        self.require_internal_abilities(holder, &format!("activate ability {}", definition.0))?;
        let object = self.object(holder)?.clone();
        let ability = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .find(|ability| ability.node.id == definition)
            .ok_or_else(|| invalid("ability is not effective for this holder"))?;
        let seat = seat_name(actor)?;
        if ability.node.kind != AbilityKind::Activated
            || object.zone != "field"
            || object.controller != seat
            || !self.internal_play_timing(actor, is_printed_quick(&ability.node))
            || object.state["silenced"] == true
            || !activation_type(self.object_type(holder)?)
            || (is_printed_quick(&ability.node) && self.object_type(holder)? != "フォロワー")
            || !self.prepared.is_empty()
        {
            return Err(unsupported("activation holder, controller or timing"));
        }
        let (face, _) = printed_origin(&ability.node.source.origin)?;
        let (card, ordinal) = self.internal_effective_print(holder)?;
        if self.printed_face_ordinal(&card, face)? != ordinal {
            return Err(invalid("printed ability belongs to another face"));
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: player_id(&object.controller)?,
        };
        self.internal_context(&context)?;
        self.check_internal_resources()?;
        let source = self.internal_card_source(holder, definition, actor)?;
        if let Some(limit) = ability.node.play.as_ref().and_then(|play| play.limit)
            && !self.card_runtime.quota_available(
                source.instance,
                TurnId(self.state.schedule.current()?),
                limit,
            )
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "activated ability quota exhausted".into(),
            ));
        }
        self.start_internal_ability(ability.node, context, source, false)
    }

    pub(in crate::game) fn check_internal_resources(&self) -> Result<()> {
        for holder in self.card_runtime.resources.keys() {
            self.internal_evolution_resource(holder)?;
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
                        Int(self.internal_card_play_cost(execution)?)
                    } else {
                        *amount
                    };
                    (
                        self.internal_pay_pp(execution.context.controller, amount)?,
                        Vec::new(),
                    )
                }
                Action::Move {
                    objects,
                    movement,
                    reason,
                } => {
                    let original = execution.object(self, &ObjectOperand::SourceHolder {})?;
                    self.internal_move(
                        execution,
                        &step.instruction,
                        objects,
                        Some((movement, *reason)),
                    )?;
                    if !matches!(execution.receipts.get(&step.instruction.receipt),
                        Some(InstructionOutcome::Move(outcome)) if matches!(outcome.moved.as_slice(), [member]
                            if member.before == original && member.after.is_some()))
                    {
                        return Err(crate::EngineFailure::IllegalPlay(
                            "incomplete movement cost".into(),
                        ));
                    }
                    (
                        Quantity {
                            amount: Int(1),
                            unit: QuantityUnit::Cards,
                        },
                        vec![original],
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
                Action::Life {
                    player: PlayerRef::Controller {},
                    change:
                        NumericChange::Add {
                            amount: Expr::Literal(Int(delta)),
                        },
                } if *delta <= 0 => (
                    self.internal_pay_life(execution, &step.instruction, *delta)?,
                    Vec::new(),
                ),
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

fn check_printed_face(printed: &serde_json::Value) -> Result<()> {
    if printed["cost"].is_null() {
        return Err(unsupported("missing printed card cost"));
    }
    if printed["card_type"] == "アミュレット" && (printed["power"] != "-" || printed["hp"] != "-")
    {
        return Err(unsupported("amulet with printed combat statistics"));
    }
    Ok(())
}

// Evolved forms carry their own printed definitions; token and advance rules are not represented.
fn activation_type(kind: &str) -> bool {
    matches!(kind, "フォロワー" | "フォロワー・エボルヴ" | "アミュレット")
}

fn check_header(ability: &Ability) -> Result<()> {
    if card_permission(ability) || printed_keywords::card_declaration(ability).is_some() {
        return Ok(());
    }
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
    if evolution::intrinsic(ability) {
        return Ok(());
    }
    if ability.kind == AbilityKind::Triggered {
        return triggers::check_header(ability);
    }
    if !matches!(ability.kind, AbilityKind::Activated | AbilityKind::Spell)
        || ability.trigger.is_some()
        || !(matches!(ability.source.origin, AbilityOrigin::Printed { .. })
            || (ability.kind == AbilityKind::Activated && is_printed_quick(ability)))
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
    if is_printed_quick(ability)
        && steps
            .iter()
            .rev()
            .skip(1)
            .any(|step| matches!(step.instruction.action, Action::Move { .. }))
    {
        return Err(unsupported("Quick source movement must finish payment"));
    }
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
            Action::Life {
                player: PlayerRef::Controller {},
                change:
                    NumericChange::Add {
                        amount: Expr::Literal(Int(amount)),
                    },
            } => *amount <= 0 && amount.checked_neg().is_some(),
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
            Action::Move {
                objects: Selector::Objects { objects },
                movement,
                reason: MoveReason::Put,
            } => {
                objects == &[ObjectOperand::SourceHolder {}]
                    && movement.to
                        == (MoveDestination::Owners {
                            zone: OwnerZone::Cemetery,
                        })
                    && movement.position.is_none()
                    && movement.order.is_none()
                    && movement.initial_posture.is_none()
                    && movement.entry_effects.is_empty()
            }
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
            keyword:
                KeywordId::Fanfare
                | KeywordId::Lastword
                | KeywordId::OnAttack
                | KeywordId::OnEvolve
                | KeywordId::OnSuperEvolve
                | KeywordId::Evolution
                | KeywordId::Quick
                | KeywordId::Ward
                | KeywordId::Aura
                | KeywordId::Storm
                | KeywordId::Rush
                | KeywordId::Bane
                | KeywordId::Intimidate
                | KeywordId::Assail,
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
    game: &Game,
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
        game.check_card_source(
            &located.source,
            &source.card_no,
            &source.face,
            u64::from(source.line),
        )?;
        match &located.node {
            Node::Instruction { instruction } => {
                super::tokens::check_catalog(game, &instruction.action)?;
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
                    } | Action::Resource { .. }
                        | Action::Discard { .. }
                        | Action::Reveal { .. }
                        | Action::Move { .. }
                        | Action::Destroy { .. }
                        | Action::Life { .. }
                        | Action::Banish { .. }
                        | Action::Posture { .. }
                        | Action::Modify { .. }
                        | Action::InstallEffect { .. }
                        | Action::Search { .. }
                        | Action::Token { .. }
                        | Action::Order { .. }
                ) {
                    return Err(unsupported("whole-card synchronous action"));
                }
            }
            Node::Control {
                control: Control::Pay { costs, on_paid, .. },
            } => {
                optional::check_costs(costs, ability_source)?;
                check_nodes(game, &on_paid.steps, source, ability_source)?;
            }
            Node::Control {
                control: Control::Sequence { body },
            } => {
                check_nodes(game, &body.steps, source, ability_source)?;
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_nodes(game, &then_body.steps, source, ability_source)?;
                if let Some(body) = else_body {
                    check_nodes(game, &body.steps, source, ability_source)?;
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    check_nodes(game, &mode.body.steps, source, ability_source)?;
                }
            }
            Node::Capture { .. } => {}
            Node::Decision { decision }
                if matches!(decision.domain, ChoiceDomain::Objects { .. }) =>
            {
                selection::check_resolution(decision)?;
            }
            Node::Control { .. } | Node::Decision { .. } => {
                return Err(unsupported("whole-card synchronous node"));
            }
        }
    }
    Ok(())
}
