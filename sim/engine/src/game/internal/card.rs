use super::super::{Step, list};
use super::{
    Context, Execution, Fact, Game, InstructionOutcome, Progress, admission, int, player_id,
    seat_name, unsupported,
};
use crate::catalog::{RuntimeCards, SelectedEffects};
use crate::ir::effect_evolution::projection;
use crate::ir::events::played::filter;
use crate::ir::play_restrictions::card_restriction;
use crate::ir::quick::{card_permission, is_printed_quick};
use crate::ir::removal_restrictions as holder_removal;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::trigger_restrictions as trigger_permission;
use crate::ir::{
    Ability, AbilityCostPolicy, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin,
    AbilityQualification, AbilitySource, Action, AssociationId, CharacteristicPatch, Control,
    CostSpec, DamageAmount, DiscardMode, Effect, ExecutionId, Expr, FaceId, Generation,
    GrantedAbility, Instruction, InstructionContext, Int, KeywordId, Located, MealRaceOperation,
    MoveDestination, MoveReason, Nat, Node, NumericChange, ObjectOperand, ObjectRef, OwnerZone,
    PlayerId, PlayerRef, Posture, Quantity, QuantityUnit, Requirements, Resource,
    ResourceOperation, RideDriveOperation, RuleId, RuntimeAbilitySource, Selector, SourceSpan,
    StepwiseMode, WellFormedCardProgram, ZoneKind,
};
use crate::ir::{Bool, ChoiceDomain, Decision, PreconditionKind, PreconditionReason};
use crate::ir::{ConsumptionPoint, LimitSpec, ModeUse, PlaySpec};
use crate::ir::{TurnId, printed_keywords, stand_restrictions};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::{mem::take, result};
use serde::Serializer;
use serde::ser::Error as _;
use serde_json::json;

pub(in crate::game) mod ability_restrictions;
pub(in crate::game) mod attack_restrictions;
mod costs;
mod effect_evolution;
mod evolution;
pub(super) mod exchange;
mod fanfare;
pub(super) mod hand_payment;
mod keywords;
pub(super) mod material;
pub(in crate::game::internal) mod nested;
pub(in crate::game) mod trigger_restrictions;
pub(super) use nested::ability::AbilityWait;
pub(super) mod optional;
pub(super) mod other_costs;
mod play_restrictions;
pub(super) mod prevention;
mod printed;
pub(super) mod quick;
mod removal_restrictions;
pub(in crate::game::internal) mod resource_entry;
mod resources;
mod reveal_cost;
mod static_keywords;
pub(in crate::game::internal) mod triggers;
pub use triggers::{PendingOption, QuotaDomain, QuotaSelection};
pub(super) mod play;
pub(super) mod recipient_targets;
pub(super) mod selection;

pub use play::{CardEntry, CardPlay};
pub use selection::TargetDomain;

#[derive(Clone, Copy)]
enum ActivationKey {
    Definition(AbilityId),
    Instance(AbilityInstanceId),
}

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
    pub(in crate::game::internal) definitions: Arc<RuntimeCards>,
    resources: BTreeMap<String, String>,
    unadmitted: BTreeSet<String>,
    pub(in crate::game::internal) instances:
        BTreeMap<(String, u64, String, usize, AbilityId), AbilityInstanceId>,
    pub(in crate::game::internal) keyword_instances:
        BTreeMap<(String, u64, super::grant::GrantOrigin, AbilityId), AbilityInstanceId>,
    pub(in crate::game::internal) granted_definitions: BTreeMap<(String, AbilityId), Ability>,
    links: BTreeMap<(u64, u64, u64, u64), AssociationId>,
    quotas: BTreeMap<(AbilityInstanceId, ConsumptionPoint), (TurnId, u64)>,
    mode_uses: Vec<ModeUse>,
    admission: Option<triggers::Admission>,
    pub(in crate::game::internal) next_instance: u64,
    entry_result: Option<CardEntry>,
    pub(in crate::game) checkpoint: Option<ExecutionId>,
    pending: Vec<triggers::PendingOccurrence>,
    pub(in crate::game::internal) question: Option<(super::DecisionView, Vec<u64>)>,
    next_pending: u64,
    rule_ids: BTreeMap<String, RuleId>,
    pub(in crate::game::internal) active_source: Option<RuntimeAbilitySource>,
    pub(in crate::game::internal) quick: Option<quick::Question>,
    pub(in crate::game::internal) quick_start: bool,
    pub(in crate::game::internal) other_costs: Vec<other_costs::InstalledCost>,
}

impl CardRuntime {
    fn quota_available(&self, instance: AbilityInstanceId, turn: TurnId, limit: LimitSpec) -> bool {
        self.quota_remaining(instance, turn, limit) > 0
    }

    fn quota_remaining(&self, instance: AbilityInstanceId, turn: TurnId, limit: LimitSpec) -> u64 {
        let used = self
            .quotas
            .get(&(instance, limit.consume_at))
            .filter(|(used_turn, _)| *used_turn == turn)
            .map_or(0, |(_, used)| *used);
        limit.maximum.0.saturating_sub(used)
    }

    fn consume_quota(
        &mut self,
        instance: AbilityInstanceId,
        turn: TurnId,
        limit: LimitSpec,
        policy: AbilityCostPolicy,
    ) -> Result<()> {
        if policy == AbilityCostPolicy::Normal && !self.quota_available(instance, turn, limit) {
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
        entry.1 = entry.1.saturating_add(1);
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
        !self.other_costs.is_empty()
            || self.definitions.cards.values().any(|card| {
                !card.card().card_play_costs.is_empty()
                    || card.card().abilities.iter().any(|a| {
                        is_printed_quick(&a.node)
                            || card_restriction(&a.node).is_some()
                            || other_costs::is_static(&a.node)
                    })
                    || card.card().abilities.iter().any(|ability| {
                        evolution::intrinsic(&ability.node)
                            || resource_entry::intrinsic(&ability.node)
                            || matches!(
                                ability.node.kind,
                                AbilityKind::Spell | AbilityKind::Triggered
                            )
                    })
            })
    }
    pub(in crate::game) const fn uses_selection(&self) -> bool {
        self.selected
    }

    pub(in crate::game) fn is_empty(&self) -> bool {
        // Active installed IR must never be omitted as an empty, saveable runtime.
        self.definitions.cards.is_empty() && !self.selected && self.other_costs.is_empty()
    }

    pub(in crate::game) fn is_loaded(&self, card: &str) -> bool {
        self.definitions.cards.contains_key(card)
    }

    pub(super) fn has_definition(&self, card: &str) -> bool {
        self.definitions.cards.contains_key(card) || self.definitions.current.contains_key(card)
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Keep the owned selection API used by existing scenario fixtures."
    )]
    pub fn from_selected_effects(
        selected: SelectedEffects,
        setup: &serde_json::Value,
        facts: &serde_json::Value,
        random: &serde_json::Value,
        seed: &str,
    ) -> Result<Self> {
        selected.check_runtime()?;
        let mut game = Self::new(Arc::clone(&selected.prototypes), setup, facts, random, seed)?;
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
        // Admission depends on the accepted definitions, not the initial position.
        game.card_runtime.definitions =
            selected.runtime_cards(|| game.prepare_current_cards(&selected))?;
        game.admit_evolution_resources()?;
        Ok(game)
    }

    fn prepare_current_cards(&mut self, selected: &SelectedEffects) -> Result<Arc<RuntimeCards>> {
        let mut definitions = RuntimeCards::default();
        let mut representatives = BTreeMap::new();
        for (number, candidate) in &selected.current {
            let card = if let Some(card) = representatives.get(&candidate.card_id) {
                Arc::clone(card)
            } else {
                let card = Arc::new(self.catalog.resolve_current(Arc::clone(candidate))?);
                representatives.insert(candidate.card_id.clone(), Arc::clone(&card));
                card
            };
            self.catalog.check_current_printing(number, &card)?;
            definitions.current.insert(number.clone(), card);
        }
        self.card_runtime.definitions = Arc::new(definitions);
        let cards = self
            .card_runtime
            .definitions
            .current
            .iter()
            .map(|(number, card)| (number.clone(), Arc::clone(card)))
            .collect::<Vec<_>>();
        for (number, card) in cards {
            self.load_internal_card(&number, &card.candidate.program)?;
        }
        Ok(Arc::clone(&self.card_runtime.definitions))
    }

    fn printed_face_ordinal(&self, card: &str, face: &FaceId) -> Result<usize> {
        self.card_runtime
            .definitions
            .current
            .get(card)
            .map_or_else(|| face_ordinal(face), |current| current.ordinal(face))
    }

    fn printed_face_id(&self, card: &str, ordinal: usize) -> Result<FaceId> {
        self.card_runtime.definitions.current.get(card).map_or_else(
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
        if let Some(current) = self.card_runtime.definitions.current.get(card) {
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
        let base_face = self.printed_face_id(card_no, 0)?;
        check_base_face(base_print, pilot_face(card, &base_face.0))?;
        costs::check(self, card_no, card)?;
        optional::check_card(card)?;
        material::check_card(card)?;
        if self.card_runtime.definitions.cards.contains_key(card_no)
            || self.typed.is_some()
            || !self.prepared.is_empty()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.quick.is_some()
        {
            return Err(invalid("duplicate card or busy internal card loader"));
        }
        match self.card_runtime.definitions.current.get(card_no) {
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
            keywords::check_coordinate(&mut origins, ordinal, &located.source, declaration)?;
            let printed = self.catalog.face(card_no, ordinal)?;
            let pilot = pilot_face(card, &face.0);
            check_ability_face(ability, printed, pilot)?;
            if ability.kind == AbilityKind::Spell {
                play::check_spell(ability, printed)?;
            }
            self.check_card_source(&located.source, card_no, &face.0, line.0)?;
            if let Some(keyword) = declaration {
                keywords::check(self, card_no, located, printed, keyword, pilot)?;
                continue;
            }
            if card_permission(ability) {
                quick::check_spell_face(self, card_no, card.card(), ordinal, base_print)?;
                continue;
            }
            if play_restrictions::check(located, ordinal)? {
                continue;
            }
            if check_static(self, located)? {
                continue;
            }
            if is_printed_quick(ability) && ordinal != 0 {
                return Err(unsupported("Quick special face"));
            }
            if evolution::intrinsic(ability) {
                evolution::check(self, located)?;
                continue;
            }
            if resource_entry::intrinsic(ability) {
                resource_entry::check(located)?;
                if !resource_entry::advance(ability) {
                    continue;
                }
            }
            if matches!(ability.kind, AbilityKind::Activated | AbilityKind::Spell) {
                check_costs(ability, &located.source)?;
            }
            let body = ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing body"))?;
            check_nodes(self, &body.steps, &located.source, &ability.source)?;
            admission::check_card(
                body,
                &located.source,
                &ability.source,
                ability.trigger.as_ref(),
            )?;
            play::check(ability, &located.source)?;
        }
        Arc::make_mut(&mut self.card_runtime.definitions)
            .cards
            .insert(card_no.into(), Arc::new(card.clone()));
        Ok(())
    }

    /// Activate one unambiguous effective definition at the active player's main timing.
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
        self.activate_internal_entry(holder, ActivationKey::Definition(definition), actor)
    }

    /// Allocate cached authority identities for effective activated definitions.
    ///
    /// # Errors
    /// Rejects an unloaded holder or unavailable text-provider definition.
    pub fn internal_activated_sources(
        &mut self,
        holder: &str,
    ) -> Result<Vec<RuntimeAbilitySource>> {
        self.internal_effective_abilities(holder)?
            .into_iter()
            .filter(|ability| ability.node.kind == AbilityKind::Activated)
            .map(|ability| self.internal_effective_source(holder, &ability))
            .collect()
    }

    /// Activate a specific effective instance, including one of several equipment providers.
    ///
    /// # Errors
    /// Rejects stale instances, controller or timing and restores the whole attempt on failure.
    pub fn activate_internal_card_instance(
        &mut self,
        holder: &str,
        instance: AbilityInstanceId,
        actor: PlayerId,
    ) -> Result<CardPlay> {
        self.activate_internal_entry(holder, ActivationKey::Instance(instance), actor)
    }

    fn activate_internal_entry(
        &mut self,
        holder: &str,
        key: ActivationKey,
        actor: PlayerId,
    ) -> Result<CardPlay> {
        if self.card_runtime.quick.is_some() {
            return Err(invalid("Quick requires its authority question"));
        }
        let before = self.clone();
        let result = self.activate_loaded_card(holder, key, actor);
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
        key: ActivationKey,
        actor: PlayerId,
    ) -> Result<CardPlay> {
        let query = match key {
            ActivationKey::Definition(definition) => format!("activate ability {}", definition.0),
            ActivationKey::Instance(instance) => {
                format!("activate ability instance {}", instance.0)
            }
        };
        self.require_internal_abilities(holder, &query)?;
        let object = self.object(holder)?.clone();
        let mut selected = None;
        for candidate in self
            .internal_effective_abilities(holder)?
            .into_iter()
            .filter(|candidate| candidate.node.kind == AbilityKind::Activated)
        {
            if matches!(key, ActivationKey::Definition(definition) if candidate.node.id != definition)
            {
                continue;
            }
            let source = self.internal_effective_source(holder, &candidate)?;
            if matches!(key, ActivationKey::Instance(instance) if source.instance != instance) {
                continue;
            }
            if selected.replace((candidate, source)).is_some() {
                return Err(invalid(
                    "ambiguous effective definition requires an ability instance",
                ));
            }
        }
        let (ability, source) =
            selected.ok_or_else(|| invalid("ability is not effective for this holder"))?;
        self.internal_require_ability_permission(holder, &ability.node)?;
        let seat = seat_name(actor)?;
        if ability.node.kind != AbilityKind::Activated
            || !matches!(
                (object.zone.as_str(), ability.node.active_zones.as_slice()),
                ("field", [ZoneKind::Field])
                    | ("ex", [ZoneKind::Ex])
                    | ("hand", [ZoneKind::Hand])
                    | ("cemetery", [ZoneKind::Cemetery])
            )
            || object.controller != seat
            || !self.internal_play_timing(actor, is_printed_quick(&ability.node))
            // Installed grants follow initial ability loss; equipment grants are filtered upstream.
            || (object.state["silenced"] == true && ability.grant.is_none())
            || !activation_type(self.object_type(holder)?, &ability.node.active_zones)
            || (is_printed_quick(&ability.node)
                && !quick_activation_type(self.object_type(holder)?, &ability.node.active_zones))
            || !self.prepared.is_empty()
        {
            return Err(unsupported("activation holder, controller or timing"));
        }
        if ability.grant.is_none() {
            let (face, _) = printed_origin(&ability.node.source.origin)?;
            let (card, ordinal) = self.internal_effective_print(holder)?;
            if self.printed_face_ordinal(&card, face)? != ordinal {
                return Err(invalid("printed ability belongs to another face"));
            }
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: player_id(&object.controller)?,
        };
        self.internal_context(&context)?;
        self.check_internal_resources()?;
        if resource_entry::intrinsic(&ability.node) {
            self.check_resource_entry_available(holder, &ability.node)?;
        }
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
        self.start_internal_ability(ability.definition.node, context, source, false)
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

    pub(super) fn internal_use_prerequisite(
        &self,
        execution: &Execution,
        costs: &CostSpec,
    ) -> Result<()> {
        if let Some(predicate) = qualification(costs)?
            && !execution.boolean(self, predicate)?.0
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "card-text use prerequisite is false".into(),
            ));
        }
        Ok(())
    }

    fn internal_card_pp_amount(
        &self,
        execution: &Execution,
        card_cost: bool,
        printed: Int,
    ) -> Result<Int> {
        if card_cost {
            self.internal_card_play_cost(execution).map(Int)
        } else {
            Ok(printed)
        }
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
        self.internal_use_prerequisite(execution, costs)?;
        self.check_reveal_cost_entry(execution, costs)?;
        for step in steps {
            self.check_keyword_cost(execution, step)?;
            if material::source_field_cost(&step.instruction.action)
                && self.object(&execution.context.source)?.zone != "field"
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "source cost requires the field".into(),
                ));
            }
            let (quantity, subjects, characters) = match &step.instruction.action {
                Action::Counter { .. } => (
                    self.internal_counter(execution, &step.instruction, true)?,
                    Vec::new(),
                    Vec::new(),
                ),
                Action::Discard { .. } => {
                    let (quantity, subjects) =
                        self.internal_hand_cost_payment(execution, &step.instruction)?;
                    (quantity, subjects, vec![])
                }
                action if reveal_payment::objects(action).is_some() => {
                    self.internal_reveal_cost_payment(execution, &step.instruction)?
                }
                Action::Resource {
                    amount: Expr::Literal(amount),
                    ..
                } => {
                    let amount = self.internal_card_pp_amount(execution, card_cost, *amount)?;
                    (
                        self.internal_pay_pp(execution.context.controller, amount)?,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                Action::Posture { objects, .. }
                | Action::Banish { objects }
                | Action::Move {
                    objects: objects @ Selector::Bound { .. },
                    ..
                } => self.internal_material_payment(execution, &step.instruction, objects)?,
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
                        vec![],
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
                    Vec::new(),
                ),
                _ => return Err(unsupported("whole-card cost step")),
            };
            self.finish_keyword_payment(execution, step)?;
            if !matches!(step.instruction.action, Action::Discard { .. }) {
                execution.receipts.insert(
                    step.instruction.receipt,
                    InstructionOutcome::Payment {
                        quantity,
                        subjects,
                        characters,
                    },
                );
            }
        }
        Ok(())
    }
}

fn pilot_face(card: &WellFormedCardProgram, face: &str) -> bool {
    card.card().abilities.iter().any(|located| {
        let ability = &located.node;
        matches!(&ability.source.origin, AbilityOrigin::Printed { face: origin, .. } if origin.0 == face)
            && ability.body.as_ref().is_some_and(|body| body.steps.iter().any(|step| {
                matches!(&step.node, Node::Instruction { instruction }
                    if matches!(instruction.action, Action::BoxPilot { .. })
                        && super::numeric::pilot::check_action(&instruction.action).is_ok())
            }))
    })
}

fn check_base_face(printed: &serde_json::Value, pilot: bool) -> Result<()> {
    check_printed_face(printed, pilot)?;
    if matches!(
        printed["card_type"].as_str(),
        Some(
            "フォロワー"
                | "スペル"
                | "スペル・トークン"
                | "スペル・アドバンス"
                | "アミュレット"
                | "アミュレット・トークン"
        )
    ) {
        super::cost::printed_play_cost(&printed["cost"])?;
    }
    Ok(())
}

fn check_ability_face(ability: &Ability, printed: &serde_json::Value, pilot: bool) -> Result<()> {
    check_printed_face(printed, pilot)?;
    if is_printed_quick(ability)
        && ability.kind == AbilityKind::Activated
        && !printed["card_type"]
            .as_str()
            .is_some_and(|kind| quick_activation_type(kind, &ability.active_zones))
    {
        return Err(unsupported("Quick activation on this printed face or zone"));
    }
    if is_printed_quick(ability)
        && ability.kind == AbilityKind::Activated
        && printed["card_type"] == "フォロワー・エボルヴ"
        && ability.play.as_ref().is_some_and(|play| {
            matches!(&play.costs, CostSpec::Ordered { steps }
                if steps.iter().any(|step| matches!(step.instruction.action,
                    Action::Move { .. } | Action::Banish { .. })
                    && !material::uses_parameter(&step.instruction.action,play)))
        })
    {
        return Err(unsupported("evolved Quick source departure cost"));
    }
    if ability.kind == AbilityKind::Activated
        && !printed["card_type"]
            .as_str()
            .is_some_and(|kind| activation_type(kind, &ability.active_zones))
    {
        return Err(unsupported(
            "activated printed face is not supported in its active zone",
        ));
    }
    Ok(())
}

fn check_printed_face(printed: &serde_json::Value, pilot: bool) -> Result<()> {
    if printed["cost"].is_null() {
        return Err(unsupported("missing printed card cost"));
    }
    if matches!(
        printed["card_type"].as_str(),
        Some("アミュレット" | "アミュレット・トークン")
    ) && !pilot
        && (printed["power"] != "-" || printed["hp"] != "-")
    {
        return Err(unsupported("amulet with printed combat statistics"));
    }
    Ok(())
}

fn quick_activation_type(kind: &str, zones: &[ZoneKind]) -> bool {
    kind == "フォロワー"
        || (zones == [ZoneKind::Field] && matches!(kind, "アミュレット" | "フォロワー・エボルヴ"))
}

// Evolved forms carry their own printed definitions.
fn activation_type(kind: &str, zones: &[ZoneKind]) -> bool {
    match zones {
        [ZoneKind::Field] => matches!(
            kind,
            "フォロワー"
                | "フォロワー・アドバンス"
                | "フォロワー・エボルヴ"
                | "フォロワー・トークン"
                | "アミュレット"
                | "アミュレット・トークン"
        ),
        [ZoneKind::Hand | ZoneKind::Cemetery] => {
            matches!(kind, "フォロワー" | "スペル" | "アミュレット")
        }
        [ZoneKind::Ex] => matches!(
            kind,
            "クレスト・トークン" | "スペル・トークン" | "アミュレット・トークン" | "アミュレット"
        ),
        _ => false,
    }
}

fn check_static(game: &Game, located: &Located<Ability>) -> Result<bool> {
    let ability = &located.node;
    if holder_removal::is_mixed_static(ability) {
        for effect in &ability.declarations {
            let mut single = located.clone();
            let mut declaration = effect.clone();
            if let Effect::Replacement { condition, .. } = &mut declaration {
                // Field-only admission can reuse the holder's public-posture proof.
                *condition = static_keywords::field_holder_posture(condition);
            }
            single.node.declarations = vec![declaration];
            if holder_removal::is_static(&single.node) {
                removal_restrictions::check(&single)?;
            } else {
                prevention::check(&single)?;
            }
        }
        return Ok(true);
    }
    if holder_removal::is_static(ability) {
        removal_restrictions::check(located)?;
        return Ok(true);
    }
    if stand_restrictions::is_static(ability) {
        super::posture::check_static(located)?;
        return Ok(true);
    }
    if trigger_permission::is_static(ability) {
        trigger_restrictions::check(located)?;
        return Ok(true);
    }
    if attack_restrictions::is_static(ability) {
        attack_restrictions::check(located)?;
        return Ok(true);
    }
    if other_costs::is_static(ability) {
        other_costs::check(located)?;
        return Ok(true);
    }
    if static_keywords::is_static(ability) {
        static_keywords::check(located)?;
        check_static_granted_definitions(game, located)?;
        return Ok(true);
    }
    if prevention::is_static(ability) {
        prevention::check(located)?;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn check_header(ability: &Ability) -> Result<()> {
    if holder_removal::is_static(ability)
        || holder_removal::is_mixed_static(ability)
        || other_costs::is_static(ability)
        || trigger_permission::is_static(ability)
        || stand_restrictions::is_static(ability)
        || attack_restrictions::is_static(ability)
        || static_keywords::is_static(ability)
        || prevention::is_static(ability)
        || card_permission(ability)
        || printed_keywords::card_declaration(ability).is_some()
        || card_restriction(ability).is_some()
    {
        return Ok(());
    }
    let zones = if ability.kind == AbilityKind::Spell {
        ZoneKind::Resolution
    } else {
        ZoneKind::Field
    };
    let zone_activation = ability.kind == AbilityKind::Activated
        && matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Ex | ZoneKind::Hand | ZoneKind::Cemetery]
        )
        && (matches!(ability.source.origin, AbilityOrigin::Printed { .. })
            || is_printed_quick(ability));
    let ex_trigger = triggers::ex_phase(ability) || triggers::ex_self_entry(ability);
    let played_trigger = filter(ability).is_some();
    let hand_discard_trigger = triggers::hand_discard(ability);
    let cemetery_entry_trigger = triggers::cemetery_entry(ability);
    let equipment_trigger = triggers::equipment_header(ability)?;
    if (ability.active_zones != [zones]
        && !zone_activation
        && !ex_trigger
        && !played_trigger
        && !hand_discard_trigger
        && !cemetery_entry_trigger
        && !equipment_trigger)
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || !ability.event_captures.is_empty()
        || !ability.declarations.is_empty()
    {
        return Err(unsupported("whole-card ability header"));
    }
    if evolution::intrinsic(ability) || resource_entry::intrinsic(ability) {
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

fn qualification(costs: &CostSpec) -> Result<Option<&Expr<Bool>>> {
    let CostSpec::Ordered { steps } = costs else {
        return Err(unsupported("whole-card cost alternatives"));
    };
    let mut predicate = None;
    for (index, step) in steps.iter().enumerate() {
        if let Some(guard) = &step.instruction.precondition {
            if index != 0
                || guard.kind != PreconditionKind::CardTextPrerequisite
                || guard.protects != step.instruction.id
                || guard.reason != (PreconditionReason::CardText {})
            {
                return Err(unsupported("whole-card use prerequisite"));
            }
            predicate = Some(&guard.predicate);
        }
    }
    Ok(predicate)
}

fn check_costs(ability: &Ability, source: &SourceSpan) -> Result<()> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing play"))?;
    check_payment(play, &ability.source, is_printed_quick(ability), source)
}

pub(super) fn check_payment(
    play: &PlaySpec,
    origin: &AbilitySource,
    quick: bool,
    source: &SourceSpan,
) -> Result<()> {
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("whole-card cost alternatives"));
    };
    if let Some(predicate) = qualification(&play.costs)? {
        super::ordering::check_condition(predicate, source)?;
    }
    if quick
        && steps.iter().rev().skip(1).any(|step| {
            matches!(
                step.instruction.action,
                Action::Move { .. }
                    | Action::Banish {
                        objects: Selector::Objects { .. }
                    }
            ) || hand_payment::source_discard(&step.instruction.action)
        })
    {
        return Err(unsupported("Quick source movement must finish payment"));
    }
    let discards = steps
        .iter()
        .filter(|step| {
            matches!(
                step.instruction.action,
                Action::Discard {
                    mode: DiscardMode::Chosen { .. },
                    ..
                }
            )
        })
        .count();
    if discards > 1 || (discards > 0 && quick) {
        return Err(unsupported("one non-Quick hand cost group"));
    }
    if quick
        && steps
            .iter()
            .any(|step| super::counters::selected_cost(&step.instruction.action).is_some())
    {
        return Err(unsupported("non-self counter Quick quote"));
    }
    for step in steps {
        let instruction = &step.instruction;
        if step.materials.is_some()
            || instruction.context != InstructionContext::Cost
            || instruction.source != *origin
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
            }
            | Action::Banish {
                objects: Selector::Objects { objects },
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
            Action::Reveal { .. } => {
                !quick && reveal_payment::selected(&instruction.action, &play.parameters)
            }
            Action::Counter { .. } => super::counters::check_cost(&instruction.action, play),
            Action::Discard { .. } => hand_payment::check_cost(&instruction.action),
            action => material::check_cost(action, play),
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

pub(in crate::game::internal) fn printed_origin(origin: &AbilityOrigin) -> Result<(&FaceId, &Nat)> {
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
                | KeywordId::AdvanceActivate
                | KeywordId::Meal
                | KeywordId::OnRace
                | KeywordId::Ride
                | KeywordId::OnDriveGain
                | KeywordId::Quick
                | KeywordId::Ward
                | KeywordId::Aura
                | KeywordId::Storm
                | KeywordId::Rush
                | KeywordId::Bane
                | KeywordId::Drain
                | KeywordId::Intimidate
                | KeywordId::Assail
                | KeywordId::SingleDrive
                | KeywordId::TwinDrive
                | KeywordId::Stack,
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

fn check_nested_nodes(
    game: &Game,
    control: &Control,
    source: &SourceSpan,
    origin: &AbilitySource,
) -> Result<()> {
    match control {
        Control::EachPlayer { stages, .. } => {
            super::tokens::check_each(control)?;
            for stage in stages {
                check_nodes(game, &stage.body.steps, source, origin)?;
            }
        }
        Control::Sequence { body } => check_nodes(game, &body.steps, source, origin)?,
        control @ Control::May { body, .. } => {
            super::alternatives::check_may(control)?;
            check_nodes(game, &body.steps, source, origin)?;
        }
        Control::If {
            then_body,
            else_body,
            ..
        } => {
            check_nodes(game, &then_body.steps, source, origin)?;
            if let Some(body) = else_body {
                check_nodes(game, &body.steps, source, origin)?;
            }
        }
        _ => return Err(unsupported("nested whole-card control")),
    }
    Ok(())
}

fn check_instruction(
    game: &Game,
    instruction: &Instruction,
    source: &SourceSpan,
    ability_source: &AbilitySource,
) -> Result<()> {
    super::tokens::check_catalog(game, &instruction.action)?;
    super::tokens::equipment::check_catalog(game, &instruction.action)?;
    super::transform::check_catalog(game, &instruction.action)?;
    nested::check(&instruction.action)?;
    nested::ability::check_action(&instruction.action)?;
    super::grant::check_action(&instruction.action)?;
    super::numeric::pilot::check_action(&instruction.action)?;
    check_grant_instruction(game, instruction, source, ability_source)?;
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
            | Action::Counter { .. }
            | Action::Discard { .. }
            | Action::Reveal { .. }
            | Action::PlayCard { .. }
            | Action::PlayAbility { .. }
            | Action::Move { .. }
            | Action::Destroy { .. }
            | Action::Life { .. }
            | Action::Banish { .. }
            | Action::Transform { .. }
            | Action::Posture { .. }
            | Action::Modify { .. }
            | Action::Grant { .. }
            | Action::InstallEffect { .. }
            | Action::Search { .. }
            | Action::Token { .. }
            | Action::Equip { .. }
            | Action::BoxPilot { .. }
            | Action::Order { .. }
            | Action::RideDrive {
                operation: RideDriveOperation::GainDrive {},
                ..
            }
            | Action::MealRace {
                operation: MealRaceOperation::Race,
                ..
            }
    ) {
        return Err(unsupported("whole-card synchronous action"));
    }
    Ok(())
}

fn check_nodes(
    game: &Game,
    nodes: &[Located<Node>],
    source: &SourceSpan,
    ability_source: &AbilitySource,
) -> Result<()> {
    for located in nodes {
        check_node_source(game, &located.source, source)?;
        match &located.node {
            Node::Instruction { instruction } => {
                check_instruction(game, instruction, source, ability_source)?;
            }
            Node::Control {
                control: control @ Control::Pay { on_paid, .. },
            } => {
                optional::check_costs(control, ability_source)?;
                check_nodes(game, &on_paid.steps, source, ability_source)?;
            }
            Node::Control {
                control: control @ Control::EachObject { body, .. },
            } if super::counters::iteration(control).is_some() => {
                super::counters::check_iteration(control)?;
                check_nodes(game, &body.steps, source, ability_source)?;
            }
            Node::Control { control } if projection(control).is_some() => {
                let projection =
                    projection(control).ok_or_else(|| invalid("missing evolution projection"))?;
                if projection.instruction.source != *ability_source {
                    return Err(invalid("foreign effect evolution source"));
                }
                check_nodes(game, &projection.body.steps[..1], source, ability_source)?;
            }
            Node::Control {
                control:
                    control @ (Control::EachPlayer { .. }
                    | Control::May { .. }
                    | Control::Sequence { .. }
                    | Control::If { .. }),
            } => check_nested_nodes(game, control, source, ability_source)?,
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    check_nodes(game, &mode.body.steps, source, ability_source)?;
                }
            }
            Node::Capture { .. } => {}
            Node::Decision { decision } => check_resolution_choice(decision)?,
            Node::Control { .. } => {
                return Err(unsupported("whole-card synchronous node"));
            }
        }
    }
    Ok(())
}

fn check_grant_instruction(
    game: &Game,
    instruction: &Instruction,
    source: &SourceSpan,
    ability_source: &AbilitySource,
) -> Result<()> {
    if let Action::Grant {
        abilities,
        provider,
        ..
    } = &instruction.action
    {
        if abilities.iter().any(|granted| {
            matches!(granted, GrantedAbility::Definition { ability }
                if ability.qualification == AbilityQualification::UnionBurst)
        }) {
            return Err(unsupported("installed granted definition cannot be UB"));
        }
        if abilities.iter().any(|granted| matches!(granted, GrantedAbility::Definition { ability } if filter(ability).is_some())) {
            return Err(unsupported("granted Play requires an equipment provider"));
        }
        check_granted_definitions(game, abilities, source, ability_source)?;
        if &provider.face != printed_origin(&instruction.source.origin)?.0 {
            return Err(unsupported("grant provider face"));
        }
    }
    Ok(())
}

fn check_node_source(game: &Game, span: &SourceSpan, source: &SourceSpan) -> Result<()> {
    if span.owner != source.owner || span.field != source.field || span.fragment != source.fragment
    {
        return Err(invalid("instruction source evidence mapping mismatch"));
    }
    game.check_card_source(span, &source.card_no, &source.face, u64::from(source.line))?;
    Ok(())
}

fn check_static_granted_definitions(game: &Game, located: &Located<Ability>) -> Result<()> {
    for effect in &located.node.declarations {
        if let Effect::Continuous {
            patch: CharacteristicPatch::GrantAbility { abilities },
            ..
        } = effect
        {
            for granted in abilities {
                if let GrantedAbility::Definition { ability } = granted
                    && (ability.active_zones != [ZoneKind::Field]
                        || static_keywords::is_static(ability)
                        || (equipment_definition(ability)
                            && located.node.active_zones != [ZoneKind::Equipment]))
                {
                    return Err(unsupported("continuous granted zone or transitive aura"));
                }
            }
            check_granted_definitions(game, abilities, &located.source, &located.node.source)?;
        }
    }
    Ok(())
}

fn equipment_definition(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Activated || filter(ability).is_some()
}

fn check_granted_static(located: &Located<Ability>) -> Result<()> {
    if prevention::is_static(&located.node) {
        prevention::check(located)
    } else if static_keywords::is_static(&located.node) {
        if located.node.declarations.iter().any(|effect| matches!(effect,
            Effect::Continuous { patch: CharacteristicPatch::GrantAbility { abilities }, .. }
                if abilities.iter().any(|ability| matches!(ability, GrantedAbility::Definition { .. })))) {
            return Err(unsupported("nested continuous definitions"));
        }
        static_keywords::check(located)
    } else if super::grant::static_cost(&located.node) {
        Ok(())
    } else {
        Err(unsupported("granted static definition"))
    }
}

fn check_granted_definitions(
    game: &Game,
    abilities: &[GrantedAbility],
    source: &SourceSpan,
    provider: &AbilitySource,
) -> Result<()> {
    for granted in abilities {
        let GrantedAbility::Definition { ability } = granted else {
            continue;
        };
        if printed_origin(&ability.source.origin)? != printed_origin(&provider.origin)? {
            return Err(invalid("granted definition has a foreign text provider"));
        }
        if !super::grant::supported_definition(ability) {
            return Err(unsupported("granted definition event"));
        }
        if ability.kind == AbilityKind::Static {
            check_granted_static(&Located {
                node: *ability.clone(),
                source: source.clone(),
            })?;
            continue;
        }
        check_header(ability)?;
        if ability.kind == AbilityKind::Activated {
            check_costs(ability, source)?;
        }
        let body = ability
            .body
            .as_ref()
            .ok_or_else(|| invalid("granted ability body"))?;
        let requirements = Requirements::derive_program(&Located {
            node: body.clone(),
            source: source.clone(),
        });
        if requirements
            .required
            .iter()
            .any(|requirement| requirement.capability.code == "action.grant")
        {
            return Err(unsupported("nested grant installation"));
        }
        check_nodes(game, &body.steps, source, &ability.source)?;
        admission::check_card(body, source, &ability.source, ability.trigger.as_ref())?;
        play::check(ability, source)?;
    }
    Ok(())
}

fn check_resolution_choice(decision: &Decision) -> Result<()> {
    match &decision.domain {
        ChoiceDomain::Abilities { .. } => nested::ability::check_choice(decision),
        ChoiceDomain::Objects { .. } | ChoiceDomain::Recipients { .. } => {
            selection::check_resolution(decision)
        }
        _ => Err(unsupported("whole-card synchronous node")),
    }
}
