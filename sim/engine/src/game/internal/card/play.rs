use super::super::{Answer, DecisionToken, DecisionView, InputDomain, Slot, Wait};
use super::{
    Ability, Activation, Context, Execution, ExecutionId, Expr, Fact, Game, Generation, Int,
    Located, Nat, Node, PlayerId, Result, ResumeStage, RuntimeAbilitySource, SourceSpan, Step,
    admission, int, invalid, seat_name, unsupported,
};
use crate::game::internal::{InstructionOutcome, UbHistoryFact, player_id};
use crate::game::legal::subsets;
use crate::ir::optional_payment;
use crate::ir::quick::is_printed_quick;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    AbilityQualification, Action, Bool, Cardinality, LimitSpec, ModeUse, ObjectId, ObjectOperand,
    ObjectRef, PaymentTime, PlaySpec, PlayerRef, ReceiptId, RecipientSet, SelectionSnapshot,
    Selector, TurnId,
};
use crate::ir::{
    ChoiceDomain, Control, CostSpec, Decision, DecisionId, DecisionKind, ExecutionRef, InputMode,
    Mode, ModeChoiceKind, ModeDecision, ModeDecisionKind, ModeId, NumericChange, PlaySlot, Program,
    Resource, ResourceOperation, ScopeId, SnapshotId, Visibility, ZoneKind,
};
use crate::ir::{FaceId, PaymentResult, PlayCostPayment, RuntimeReceiptId};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use core::slice::from_ref;
use serde_json::{Value, json};

/// A formal play exposes a question or a fully committed result, never partial receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardPlay {
    /// Answer through `answer_internal_card` using only this token and a payload.
    Awaiting(DecisionView),
    /// Complete printed Spell or ability, followed by explicit check timing.
    Resolved(Activation),
    /// Complete follower or amulet play, followed by its separately produced fanfare.
    Entered(CardEntry),
    /// One pending occurrence cannot legally be played and has been cancelled by rules.
    Cancelled,
    /// A valid Quick Pass resumed the stored attack or end-phase procedure.
    Continued(Step),
}

/// Complete follower or amulet rule play; fanfare remains a separate execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardEntry {
    /// Authority producer for the card play.
    pub execution: ExecutionId,
    /// Complete intrinsic payment and card facts.
    pub progress: super::super::Progress,
    /// Committed causal movement and play events.
    pub step: Step,
    /// The authority-produced fanfare follows the completed rule check.
    pub resume: ResumeStage,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ModeWait {
    pub(in crate::game::internal) chosen: ModeDecision,
    pub(in crate::game::internal) count: Nat,
}

#[derive(Clone, Debug)]
struct PlayMode {
    modes: Vec<Mode>,
    count: Expr<Nat>,
    chosen: ModeDecision,
    condition: Option<Expr<Bool>>,
}

impl PlayMode {
    fn parameters(&self) -> Result<Vec<Decision>> {
        let mut parameters = Vec::new();
        if matches!(
            &self.chosen.domain,
            ChoiceDomain::Modes {
                cardinality: Cardinality {
                    minimum: Nat(0),
                    ..
                },
                ..
            }
        ) && self.count != Expr::Literal(Nat(0))
        {
            return Err(unsupported("any-number mode choice"));
        }
        for mode in &self.modes {
            for decision in mode
                .parameters
                .iter()
                .filter(|decision| super::material::is_parameter(decision))
            {
                super::material::check_parameter(decision)?;
            }
            if mode
                .parameters
                .iter()
                .filter(|decision| !super::material::is_parameter(decision))
                .any(|decision| !independent_mode_target(decision))
            {
                return Err(unsupported(
                    "mode targets require independent public selectors",
                ));
            }
            parameters.extend(mode.parameters.clone());
        }
        Ok(parameters)
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct PlayState {
    qualification: AbilityQualification,
    pub(in crate::game::internal) resource_entry: Option<super::resource_entry::ResourceEntryState>,
    snapshot: Option<Box<Game>>,
    pub(in crate::game::internal) snapshot_id: SnapshotId,
    modes: VecDeque<PlayMode>,
    groups: Vec<PlayMode>,
    pub(in crate::game::internal) chosen: BTreeMap<DecisionId, Vec<ModeId>>,
    parameters: VecDeque<Decision>,
    pub(super) exchange: Option<super::exchange::ExchangeState>,
    pub(super) optional: Vec<super::optional::OptionalState>,
    pub(super) costs: CostSpec,
    pub(super) additional: Option<Box<(FaceId, PlayCostPayment)>>,
    mode_costs: CostSpec,
    limit: Option<LimitSpec>,
    body: Program,
    restart: Option<SelectionOrigin>,
    distribution_captures: Option<Decision>,
}

#[derive(Clone, Debug)]
struct SelectionOrigin {
    slots: BTreeMap<(ScopeId, u64), Slot>,
    parameters: VecDeque<Decision>,
}

impl PlayState {
    fn material_posture(&self, decision: &Decision) -> bool {
        super::material::posture_parameter(&self.costs, decision)
            || self
                .optional
                .iter()
                .any(|payment| super::material::posture_parameter(&payment.costs, decision))
            || self.additional.as_deref().is_some_and(|(_, payment)| {
                super::material::posture_parameter(&payment.play.costs, decision)
            })
    }

    fn declined_material(&self, decision: &Decision) -> bool {
        let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
            return false;
        };
        self.optional.iter().any(|payment| {
            payment.accepted == Some(false)
                && matches!(&payment.costs, CostSpec::Ordered { steps } if steps.iter().any(|step| {
                    let action = &step.instruction.action;
                    matches!(super::material::cost_objects(action).or_else(|| reveal_payment::objects(action)), Some(Selector::Bound { reference }) if reference == binding)
                        || super::super::counters::selected_cost(action) == Some(*binding)
                }))
        })
    }

    fn freeze_selection(&mut self, game: &Game, execution: &Execution) -> Result<()> {
        if self.snapshot.is_none() {
            for group in &self.groups {
                let chosen = self
                    .chosen
                    .get(&group.chosen.id)
                    .ok_or_else(|| invalid("missing modes"))?;
                for mode in &group.modes {
                    if chosen.contains(&mode.id) {
                        self.parameters.extend(mode.parameters.clone());
                        self.optional
                            .extend(super::optional::mode_state(&mode.body));
                        let CostSpec::Ordered { steps: mode_steps } = &mode.costs else {
                            return Err(unsupported("mode cost alternatives"));
                        };
                        let CostSpec::Ordered { steps: selected } = &mut self.mode_costs else {
                            return Err(unsupported("selected mode costs"));
                        };
                        selected.extend(mode_steps.clone());
                    }
                }
            }
            // Cost materials of every selected mode follow all formal targets (R-0006).
            // Without mode merging, additional-cost materials retain their earlier question.
            if !self.groups.is_empty() {
                self.parameters
                    .make_contiguous()
                    .sort_by_key(super::material::is_parameter);
            }
            // The first target fixes the game view; later answers only extend immutable slots.
            self.snapshot_id = SnapshotId(game.state.ir_epoch);
            self.snapshot = Some(Box::new(game.clone()));
            self.restart = Some(SelectionOrigin {
                slots: execution.slots.clone(),
                parameters: self.parameters.clone(),
            });
            // Never ask a target question that no target and distribution plan can satisfy.
            if self
                .parameters
                .iter()
                .any(|decision| matches!(decision.domain, ChoiceDomain::Distribution { .. }))
                && !game.internal_feasible_targets(execution, &execution.slots, &self.parameters)?
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "no legal target and distribution plan".into(),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn has_mode_payments(body: &Program) -> Result<bool> {
    Ok(collect_modes(body, true)?.iter().any(|group| {
        group
            .modes
            .iter()
            .any(|mode| optional_payment::mode_declaration(&mode.body).is_some())
    }))
}

pub(super) fn check_additional_body(ability: &Ability) -> Result<()> {
    let body = ability
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing Spell body"))?;
    if !collect_modes(body, conditional_play_modes(ability))?.is_empty()
        || optional_payment::declaration(body).is_some()
    {
        return Err(unsupported(
            "additional with modes or optional head payment",
        ));
    }
    Ok(())
}

pub(super) fn check_spell(ability: &Ability, printed: &Value) -> Result<()> {
    let Some(play) = &ability.play else {
        return Err(unsupported("Spell play"));
    };
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("Spell costs"));
    };
    let [step] = steps.as_slice() else {
        return Err(unsupported("Spell needs one intrinsic PP payment"));
    };
    let Action::Resource {
        amount: Expr::Literal(Int(amount)),
        ..
    } = &step.instruction.action
    else {
        return Err(unsupported("Spell intrinsic cost"));
    };
    if !matches!(
        printed["card_type"].as_str(),
        Some("スペル" | "スペル・トークン" | "スペル・アドバンス")
    ) || *amount != super::super::cost::printed_play_cost(&printed["cost"])?
    {
        return Err(unsupported(
            "Spell catalogue kind or intrinsic cost mismatch",
        ));
    }
    Ok(())
}

pub(super) fn check(ability: &Ability, source: &SourceSpan) -> Result<()> {
    let Some(play) = &ability.play else {
        return Ok(());
    };
    let body = ability
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing body"))?;
    let groups = collect_modes(body, conditional_play_modes(ability))?;
    if is_printed_quick(ability)
        && groups.iter().any(|group| {
            matches!(
                &group.chosen.domain,
                ChoiceDomain::Modes {
                    exclude_used: Some(_),
                    ..
                }
            )
        })
    {
        return Err(unsupported(
            "Quick mode exclusion needs a history-aware preflight",
        ));
    }
    let mut parameters = play.parameters.clone();
    for group in &groups {
        parameters.extend(group.parameters()?);
        for mode in &group.modes {
            let CostSpec::Ordered { steps: mode_steps } = &mode.costs else {
                return Err(unsupported("mode cost alternatives"));
            };
            if mode
                .parameters
                .iter()
                .filter(|decision| super::material::is_parameter(decision))
                .count()
                > 1
            {
                return Err(unsupported("one material group per mode"));
            }
            if !mode_steps.is_empty() && optional_payment::mode_declaration(&mode.body).is_some() {
                return Err(unsupported("one mode payment declaration"));
            }
            if mode_steps.iter().any(|step| !matches!(&step.instruction.action,
                Action::Resource { player: PlayerRef::Controller {}, resource: Resource::PpCurrent,
                    operation: ResourceOperation::Pay, amount: Expr::Literal(Int(amount)) } if *amount >= 0)) {
                return Err(unsupported("mode PP payment"));
            }
            let mut candidate = ability.clone();
            let definition = candidate
                .play
                .as_mut()
                .ok_or_else(|| invalid("mode play"))?;
            let costs = optional_payment::mode_declaration(&mode.body)
                .map_or(&mode.costs, |(costs, _, _)| costs);
            if matches!(costs, CostSpec::Ordered { steps } if steps.iter().any(|step| super::material::source_field_cost(&step.instruction.action)))
                && ability.active_zones != [ZoneKind::Field]
            {
                return Err(unsupported("mode source cost needs the field"));
            }
            definition.costs = costs.clone();
            definition.parameters.clone_from(&mode.parameters);
            // Declining a mode fee is always available, so it needs no Quick payment quote.
            super::check_payment(definition, &ability.source, false, source)?;
        }
    }
    check_overlapping_mode_materials(play, &groups)?;
    parameters.sort_by_key(super::material::is_parameter);
    check_parameters(&parameters)?;
    let mut steps = play
        .captures
        .iter()
        .cloned()
        .map(|capture| Located {
            node: Node::Capture { capture },
            source: source.clone(),
        })
        .collect::<Vec<_>>();
    steps.extend(parameters.into_iter().map(|decision| Located {
        node: Node::Decision { decision },
        source: source.clone(),
    }));
    admission::check_play(
        &Program {
            scope: ability.scope,
            steps,
        },
        source,
    )
}

/// Selected cost materials share one pool, so only one chosen source of them is admitted.
fn check_overlapping_mode_materials(play: &PlaySpec, groups: &[PlayMode]) -> Result<()> {
    let chosen_hand = |costs: &CostSpec| {
        matches!(costs, CostSpec::Ordered { steps } if steps.iter().any(|step|
            matches!(step.instruction.action, Action::Discard { .. })
                && !super::hand_payment::source_discard(&step.instruction.action)))
    };
    let fee = |mode: &Mode| {
        mode.parameters.iter().any(super::material::is_parameter)
            || optional_payment::mode_declaration(&mode.body)
                .is_some_and(|(costs, _, _)| chosen_hand(costs))
    };
    let mut paying_groups = 0_usize;
    for group in groups {
        let fees = group.modes.iter().filter(|mode| fee(mode)).count();
        let single = matches!(&group.chosen.domain, ChoiceDomain::Modes { cardinality, .. }
            if cardinality.maximum.is_some_and(|maximum| maximum.0 <= 1))
            || matches!(group.count, Expr::Literal(Nat(count)) if count <= 1);
        if fees > 1 && !single {
            return Err(unsupported("several selectable mode material fees"));
        }
        paying_groups = paying_groups.saturating_add(usize::from(fees > 0));
    }
    if paying_groups > 1
        || (paying_groups == 1
            && (play.parameters.iter().any(super::material::is_parameter)
                || chosen_hand(&play.costs)))
    {
        return Err(unsupported(
            "mode material fee beside another material cost",
        ));
    }
    Ok(())
}

fn check_parameters(parameters: &[Decision]) -> Result<()> {
    let mut distribution = false;
    let mut material = false;
    let mut target = None;
    let mut target_count = 0_usize;
    for decision in parameters {
        if super::material::is_parameter(decision) {
            super::material::check_parameter(decision)?;
            material = true;
            continue;
        }
        if material {
            return Err(unsupported("targets must precede materials"));
        }
        match (&decision.kind, &decision.domain) {
            (
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Targets,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                ChoiceDomain::Objects {
                    constraint,
                    binding,
                    target_roles: None,
                },
            ) if !distribution => {
                super::selection::check(constraint)?;
                target = Some(*binding);
                target_count = target_count.saturating_add(1);
            }
            (
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Distribution,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                ChoiceDomain::Distribution {
                    recipients:
                        RecipientSet::Objects {
                            objects: Selector::Bound { reference },
                        },
                    minimum: Nat(1),
                    ..
                },
            ) if Some(*reference) == target => {
                distribution = true;
            }
            (
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Targets,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                ChoiceDomain::Recipients {
                    candidates,
                    cardinality,
                    ..
                },
            ) if !distribution => {
                super::recipient_targets::check(candidates, *cardinality)?;
                target = None;
                target_count = target_count.saturating_add(1);
                if !decision.captures.is_empty() {
                    return Err(unsupported("recipient target captures"));
                }
            }
            _ => return Err(unsupported("play parameter order, dependencies or domain")),
        }
        if decision.actor != (PlayerRef::Controller {})
            || decision.visibility != (Visibility::Public {})
        {
            return Err(unsupported("play parameter actor or visibility"));
        }
    }
    if distribution && target_count != 1 {
        return Err(unsupported("distribution needs exactly one target group"));
    }
    Ok(())
}

fn conditional_play_modes(ability: &Ability) -> bool {
    ability.play.as_ref().is_some_and(|play| {
        matches!(play,
            PlaySpec { limit: None, captures, parameters, costs: CostSpec::Ordered { steps } }
            if captures.is_empty() && parameters.is_empty() && steps.is_empty())
    }) && ability
        .body
        .as_ref()
        .is_some_and(|body| optional_payment::declaration(body).is_none())
}

fn collect_modes(program: &Program, conditional: bool) -> Result<Vec<PlayMode>> {
    let mut groups = Vec::new();
    for node in &program.steps {
        match &node.node {
            Node::Control {
                control: Control::Sequence { body },
            } => groups.extend(collect_modes(body, conditional)?),
            Node::Control {
                control:
                    Control::If {
                        condition,
                        then_body,
                        else_body,
                    },
            } => {
                groups.extend(conditional_modes(
                    condition,
                    then_body,
                    else_body.as_ref(),
                    conditional,
                )?);
            }
            Node::Control {
                control:
                    Control::Mode {
                        modes,
                        count,
                        chosen,
                        choice_kind: ModeChoiceKind::ChoiceKeyword,
                    },
            } if matches!(
                chosen.kind,
                ModeDecisionKind::PlayParameter {
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline
                }
            ) && chosen.actor == (PlayerRef::Controller {}) =>
            {
                groups.push(PlayMode {
                    modes: modes.clone(),
                    count: count.clone(),
                    chosen: chosen.clone(),
                    condition: None,
                });
            }
            Node::Control {
                control:
                    Control::Mode {
                        choice_kind: ModeChoiceKind::ResolutionAlternative,
                        ..
                    },
            } => {}
            Node::Control {
                control: Control::Mode { .. },
            } => return Err(unsupported("play mode timing")),
            _ => {}
        }
    }
    Ok(groups)
}

fn conditional_modes(
    condition: &Expr<Bool>,
    then_body: &Program,
    else_body: Option<&Program>,
    allowed: bool,
) -> Result<Vec<PlayMode>> {
    let mut groups = collect_modes(then_body, false)?;
    let other = else_body.map_or_else(|| Ok(Vec::new()), |body| collect_modes(body, false))?;
    if groups.is_empty() && other.is_empty() {
        return Ok(groups);
    }
    let public_condition = optional_payment::public_mode_guard(condition);
    if !allowed || groups.len() != 1 || !other.is_empty() || !public_condition {
        return Err(unsupported("conditional play mode"));
    }
    for group in &mut groups {
        group.condition = Some(condition.clone());
    }
    Ok(groups)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal plays share the authority's execution and rollback boundary."
)]
impl Game {
    pub(in crate::game::internal) fn internal_quick_preflight(
        &self,
        holder: &str,
        ability: &Ability,
        actor: PlayerId,
        spell: bool,
    ) -> Result<bool> {
        // Allocate evaluation slots on a disposable authority, without moving cards or running effects.
        let mut probe = self.clone();
        let mut execution = probe.new_card_execution(Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller: actor,
        })?;
        let play = ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Quick PlaySpec"))?;
        execution.spell = spell;
        let material = play.parameters.iter().any(super::material::is_parameter);
        if material || super::qualification(&play.costs)?.is_some() {
            let source = probe.internal_card_source(holder, ability.id, actor)?;
            execution.bind_source(&probe, source)?;
            probe.card_runtime.active_source = Some(source);
            match probe.internal_use_prerequisite(&execution, &play.costs) {
                Ok(()) => {}
                Err(crate::EngineFailure::IllegalPlay(_)) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        execution.captures(&probe, &play.captures)?;
        let available = if material {
            probe.internal_quick_material_costs_available(&execution, play)?
        } else {
            self.quick_costs_available(holder, &execution, &play.costs, actor, spell)?
        };
        if !available {
            return Ok(false);
        }
        if spell && !self.quick_additional_costs_available(holder, ability, actor, &execution)? {
            return Ok(false);
        }
        let groups = collect_modes(
            ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing Quick body"))?,
            false,
        )?;
        let targets = play
            .parameters
            .iter()
            .filter(|decision| !super::material::is_parameter(decision))
            .cloned()
            .collect();
        self.quick_mode_plan(&execution, &groups, targets)
    }

    fn quick_costs_available(
        &self,
        holder: &str,
        execution: &Execution,
        costs: &CostSpec,
        actor: PlayerId,
        spell: bool,
    ) -> Result<bool> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("Quick costs"));
        };
        let mut pp = int(&self.player(seat_name(actor)?)?.pp["current"]);
        let mut life = int(&self.player(seat_name(actor)?)?.leader["life"]);
        let mut standing = self.object(holder)?.state["acted"] == false;
        let mut counters = BTreeMap::new();
        for step in steps {
            match &step.instruction.action {
                Action::Resource {
                    amount: Expr::Literal(Int(amount)),
                    ..
                } => {
                    let amount = if spell {
                        self.internal_quote_card_cost(execution)?
                    } else {
                        *amount
                    };
                    if pp < amount {
                        return Ok(false);
                    }
                    pp = pp
                        .checked_sub(amount)
                        .ok_or_else(|| invalid("PP quote overflow"))?;
                }
                Action::Counter {
                    holder: counter_holder,
                    kind,
                    amount: Expr::Literal(amount),
                    ..
                } => {
                    let key = self.catalog.keyword_id(&kind.0);
                    let count = counters
                        .entry(key)
                        .or_insert(execution.counter(self, counter_holder, kind)?.0);
                    if *count < amount.0 {
                        return Ok(false);
                    }
                    *count = count
                        .checked_sub(amount.0)
                        .ok_or_else(|| invalid("counter quote overflow"))?;
                }
                Action::Posture { .. } => {
                    if !standing {
                        return Ok(false);
                    }
                    standing = false;
                }
                Action::Life {
                    change:
                        NumericChange::Add {
                            amount: Expr::Literal(Int(delta)),
                        },
                    ..
                } => {
                    life = life
                        .checked_add(*delta)
                        .ok_or_else(|| invalid("life quote overflow"))?;
                    if life < 0 {
                        return Ok(false);
                    }
                }
                Action::Discard { .. }
                    if super::hand_payment::source_discard(&step.instruction.action) =>
                {
                    let object = self.object(holder)?;
                    if object.zone != "hand" || object.owner != seat_name(actor)? {
                        return Ok(false);
                    }
                }
                Action::Banish {
                    objects: Selector::Objects { objects },
                } if objects == &[ObjectOperand::SourceHolder {}] => {
                    let object = self.object(holder)?;
                    if !matches!(object.zone.as_str(), "field" | "ex" | "cemetery")
                        || object.owner != seat_name(actor)?
                    {
                        return Ok(false);
                    }
                }
                Action::Move { .. } => {}
                _ => return Err(unsupported("Quick cost proof")),
            }
        }
        Ok(true)
    }

    fn quick_additional_costs_available(
        &self,
        holder: &str,
        ability: &Ability,
        actor: PlayerId,
        execution: &Execution,
    ) -> Result<bool> {
        let Some((_, payment)) = self.internal_additional(execution)?.as_deref().cloned() else {
            return Ok(true);
        };
        if !super::hand_payment::single_additional(&payment.play.costs) {
            return Err(unsupported("Quick additional hand cost"));
        }
        let mut probe = self.clone();
        // Enter Resolution through the formal play boundary before looking for hand costs.
        let mut selection = probe.spell_execution(holder, actor, ability)?;
        probe.card_runtime.active_source = selection.ability;
        let play = ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Quick play"))?;
        match probe.internal_card_payment(&mut selection, &play.costs, true) {
            Ok(()) => {}
            Err(crate::EngineFailure::IllegalPlay(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        match probe.internal_hand_cost_materials(&mut selection, &payment.play.costs) {
            Ok(true) => {}
            Ok(false) => return Err(invalid("missing Quick hand cost")),
            Err(crate::EngineFailure::IllegalPlay(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        let Some((question, Wait::CostDiscard(wait))) = selection.waiting.take() else {
            return Err(invalid("missing Quick hand cost question"));
        };
        let InputDomain::CostMaterials(domain) = question.domain else {
            return Err(invalid("Quick hand cost domain"));
        };
        for candidate in domain.members {
            let mut trial = probe.clone();
            let mut paid = selection.clone();
            let members = wait.validate(&trial, &paid, &[candidate.handle])?;
            paid.cost_discards.insert(wait.receipt(), members);
            match trial.internal_card_payment(&mut paid, &payment.play.costs, false) {
                Ok(()) => return Ok(true),
                Err(crate::EngineFailure::IllegalPlay(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    fn quick_mode_plan(
        &self,
        execution: &Execution,
        groups: &[PlayMode],
        parameters: VecDeque<Decision>,
    ) -> Result<bool> {
        let Some((group, rest)) = groups.split_first() else {
            return self.internal_feasible_targets(execution, &execution.slots, &parameters);
        };
        let count = execution.natural(self, &group.count)?;
        let ChoiceDomain::Modes { cardinality, .. } = &group.chosen.domain else {
            return Err(unsupported("Quick mode domain"));
        };
        if count == Nat(0) {
            return self.quick_mode_plan(execution, rest, parameters);
        }
        let names = group
            .modes
            .iter()
            .enumerate()
            .map(|(i, _)| i.to_string())
            .collect::<Vec<_>>();
        let maximum = cardinality.maximum.map_or(count.0, |n| n.0.min(count.0));
        for selection in subsets(
            &names,
            usize::try_from(cardinality.minimum.0).map_err(invalid)?,
            usize::try_from(maximum).map_err(invalid)?,
        ) {
            let selected = group
                .modes
                .iter()
                .enumerate()
                .filter(|(index, _)| selection.contains(&index.to_string()))
                .map(|(_, mode)| mode.id)
                .collect::<Vec<_>>();
            if !group.chosen.accepts_answer(count, &selected) {
                continue;
            }
            let mut parameters = parameters.clone();
            for (index, mode) in group.modes.iter().enumerate() {
                if selection.contains(&index.to_string()) {
                    parameters.extend(
                        mode.parameters
                            .iter()
                            .filter(|decision| !super::material::is_parameter(decision))
                            .cloned(),
                    );
                }
            }
            if self.quick_mode_plan(execution, rest, parameters)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Play an admitted Spell, follower or amulet.
    ///
    /// # Errors
    /// Rejects unsupported cards, timing, carry, payment and body failures atomically.
    pub fn play_internal_card(&mut self, holder: &str, actor: PlayerId) -> Result<CardPlay> {
        self.require_internal_abilities(holder, "card play")?;
        if self.card_runtime.quick.is_some() {
            return Err(invalid("Quick requires its authority question"));
        }
        let before = self.clone();
        let result = if matches!(
            self.object_type(holder)?,
            "スペル" | "スペル・トークン" | "スペル・アドバンス"
        ) {
            self.play_internal_spell(holder, actor)
        } else {
            self.play_internal_field_card(holder, actor)
        };
        self.finish_card_attempt(before, result)
    }

    pub(super) fn play_internal_spell(
        &mut self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<CardPlay> {
        let mut execution = self.prepare_card_execution(holder, actor, None)?;
        self.internal_run(&mut execution)?;
        self.card_play_result(execution)
    }

    pub(super) fn spell_execution(
        &mut self,
        holder: &str,
        actor: PlayerId,
        ability: &Ability,
    ) -> Result<Execution> {
        let object = self.object(holder)?.clone();
        let old_context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        };
        let mut execution = self.new_card_execution(old_context)?;
        execution.play_origin_player = Some(player_id(&object.controller)?);
        let play = ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Spell play"))?;
        execution.spell = true;
        execution.captures(self, &play.captures)?;
        self.internal_play_move(holder, "resolution", &execution.frame())?;
        self.object_mut(holder)?.state["entered_from"] = json!(object.zone);
        execution.context.generation = Generation(self.object(holder)?.generation);
        let source = self.internal_card_source(holder, ability.id, actor)?;
        // The Spell's source is its Resolution generation, never the Hand/EX play-time object.
        execution.bind_source(self, source)?;
        Ok(execution)
    }

    pub(super) fn check_card_play_holder(&self, holder: &str, actor: PlayerId) -> Result<()> {
        let object = self.object(holder)?;
        let seat = seat_name(actor)?;
        self.internal_context(&Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        })?;
        if object.owner != seat
            || object.controller != seat
            || !matches!(object.zone.as_str(), "hand" | "ex")
            || !self.internal_play_timing(actor, self.internal_quick_spell(holder)?)
            || int(&object.state["face"]) != 0
            || !self.prepared.is_empty()
            || object.state["evolved_with"].is_string()
            || self.restricted(holder, "play")?
            || self.internal_card_play_forbidden(holder, actor)?
        {
            return Err(unsupported("card play holder, permission or timing"));
        }
        self.check_internal_resources()
    }

    pub(super) fn start_internal_ability(
        &mut self,
        ability: Ability,
        context: Context,
        source: RuntimeAbilitySource,
        spell: bool,
    ) -> Result<CardPlay> {
        let mut execution = self.new_card_execution(context)?;
        execution.bind_source(self, source)?;
        execution.spell = spell;
        if let Some(play) = &ability.play {
            self.internal_use_prerequisite(&execution, &play.costs)?;
            execution.captures(self, &play.captures)?;
        }
        self.run_card_play(execution, ability)
    }

    fn internal_play_counts(&self) -> Result<BTreeMap<u32, (Nat, Nat)>> {
        let mut counts = BTreeMap::new();
        for seat in [1, 2] {
            let cards = self.zone_ids(seat_name(PlayerId { seat })?, "cemetery");
            let mut spells = 0_u64;
            for id in &cards {
                if self.object_type(id)? == "スペル" {
                    spells = spells
                        .checked_add(1)
                        .ok_or_else(|| invalid("SpellChain count overflow"))?;
                }
            }
            counts.insert(
                seat,
                (
                    Nat(spells),
                    Nat(u64::try_from(cards.len()).map_err(invalid)?),
                ),
            );
        }
        Ok(counts)
    }

    pub(super) fn new_card_execution(&mut self, context: Context) -> Result<Execution> {
        self.emitted.clear();
        self.fresh_card_execution(context)
    }

    pub(super) fn fresh_card_execution(&mut self, context: Context) -> Result<Execution> {
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        self.state.ir_play_clock = self
            .state
            .ir_play_clock
            .checked_add(1)
            .ok_or_else(|| invalid("proper play time exhausted"))?;
        let ids = self.internal_identities()?;
        self.card_execution(context, ids)
    }

    pub(in crate::game::internal) fn card_execution(
        &self,
        context: Context,
        ids: BTreeMap<ObjectId, String>,
    ) -> Result<Execution> {
        let play_origin =
            self.state
                .objects
                .get(&context.source)
                .and_then(|object| match object.zone.as_str() {
                    "hand" => Some(ZoneKind::Hand),
                    "ex" => Some(ZoneKind::Ex),
                    _ => None,
                });
        Ok(Execution {
            context,
            id: ExecutionId(self.state.ir_epoch),
            timestamp: self.state.ir_play_clock,
            ids,
            todo: VecDeque::new(),
            slots: BTreeMap::new(),
            receipts: BTreeMap::new(),
            cost_discards: BTreeMap::new(),
            material_selections: BTreeMap::new(),
            effect_evolution_destinations: BTreeMap::new(),
            facts: Vec::new(),
            waiting: None,
            destroy_guards: Vec::new(),
            look_viewers: BTreeMap::new(),
            reveals: Vec::new(),
            allocations: BTreeMap::new(),
            origin: None,
            ability: None,
            source_information: None,
            event_object: None,
            attack_event: None,
            entry_method: None,
            entry_origin: None,
            damage_event: None,
            leave_move: None,
            targets: BTreeMap::new(),
            play: None,
            spell: false,
            field_entry: false,
            play_origin,
            card_payment: None,
            trigger_occurrence: None,
            play_counts: self.internal_play_counts()?,
            ward_entries: Vec::new(),
            creating: None,
            nested: None,
            cost_override: None,
            play_origin_player: None,
        })
    }

    pub(super) fn run_card_play(
        &mut self,
        mut execution: Execution,
        ability: Ability,
    ) -> Result<CardPlay> {
        self.prepare_ability_play(&mut execution, ability)?;
        self.internal_run(&mut execution)?;
        self.card_play_result(execution)
    }

    pub(super) fn prepare_ability_play(
        &self,
        execution: &mut Execution,
        ability: Ability,
    ) -> Result<()> {
        let conditional = conditional_play_modes(&ability);
        let resource_entry = super::resource_entry::payment(&ability)?;
        let mut body = ability.body.ok_or_else(|| invalid("missing body"))?;
        let optional = super::optional::take(&mut body);
        let groups = collect_modes(&body, conditional)?;
        let play = ability.play.unwrap_or(PlaySpec {
            limit: None,
            captures: Vec::new(),
            parameters: Vec::new(),
            costs: CostSpec::Ordered { steps: Vec::new() },
        });
        let additional = if execution.spell {
            self.internal_additional(execution)?
        } else {
            None
        };
        let parameters = additional
            .as_deref()
            .into_iter()
            .flat_map(|(_, payment)| payment.play.parameters.clone())
            .chain(if resource_entry.is_some() {
                vec![]
            } else {
                play.parameters
            })
            .collect();
        execution.play = Some(PlayState {
            qualification: ability.qualification,
            additional,
            resource_entry,
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: groups.clone().into(),
            groups,
            chosen: BTreeMap::new(),
            parameters,
            exchange: if execution.spell {
                self.internal_exchange(execution)?
            } else {
                None
            },
            optional: optional.into_iter().collect(),
            costs: play.costs,
            mode_costs: CostSpec::Ordered { steps: vec![] },
            limit: play.limit,
            body,
            restart: None,
            distribution_captures: None,
        });
        Ok(())
    }

    pub(super) fn finish_card_attempt(
        &mut self,
        before: Self,
        result: Result<CardPlay>,
    ) -> Result<CardPlay> {
        if result.is_err() {
            let high = self.state.ir_epoch;
            *self = before;
            if self.card_runtime.quick.is_some() {
                self.state.ir_epoch = self.state.ir_epoch.max(high);
                self.refresh_internal_quick()?;
            }
            return result;
        }
        if let Some(execution) = self.typed.as_mut() {
            execution.origin = Some(Box::new(before));
        }
        result
    }

    pub(super) fn card_play_result(&mut self, execution: Execution) -> Result<CardPlay> {
        if let Some((view, _)) = &execution.waiting {
            let view = view.clone();
            self.typed = Some(execution);
            Ok(CardPlay::Awaiting(view))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            activation(execution.progress(), execution.ability).map(CardPlay::Resolved)
        }
    }

    /// Answer the authority's pending formal play, without supplying execution context.
    ///
    /// # Errors
    /// Wrong answers preserve the question; commit failures restore the entire attempt.
    pub fn answer_internal_card(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
    ) -> Result<CardPlay> {
        if self.card_runtime.quick.is_some() {
            return self.answer_internal_quick(actor, token, &answer);
        }
        if self.typed.is_none() && self.card_runtime.question.is_some() {
            return self.answer_internal_pending(actor, token, &answer);
        }
        if self
            .typed
            .as_ref()
            .is_none_or(|execution| execution.ability.is_none() && !execution.field_entry)
        {
            return Err(invalid("no pending formal play"));
        }
        if self.decline_internal_head_payment(actor, token, &answer)? {
            return Ok(CardPlay::Cancelled);
        }
        let occurrence = self
            .typed
            .as_ref()
            .filter(|execution| {
                execution
                    .play
                    .as_ref()
                    .and_then(|play| play.optional.iter().find(|payment| payment.required))
                    .is_some_and(|payment| payment.required)
            })
            .and_then(|execution| execution.trigger_occurrence.map(|id| (id, execution.id)));
        let source = self.typed.as_ref().and_then(Execution::root_ability);
        let progress = match self.answer_internal_step(actor, token, answer) {
            Err(crate::EngineFailure::IllegalPlay(_)) if occurrence.is_some() => {
                let (id, checkpoint) = occurrence.ok_or_else(|| invalid("missing occurrence"))?;
                self.internal_cancel_occurrence(id, checkpoint);
                return Ok(CardPlay::Cancelled);
            }
            result => result?,
        };
        if let Some((view, _)) = self
            .typed
            .as_ref()
            .and_then(|execution| execution.waiting.as_ref())
        {
            Ok(CardPlay::Awaiting(view.clone()))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            activation(progress, source).map(CardPlay::Resolved)
        }
    }

    fn internal_play_material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        play: &PlayState,
    ) -> Result<()> {
        if reveal_payment::parameter(decision).is_some() {
            if let Some((_, additional)) = play.additional.as_deref() {
                self.internal_reveal_material_parameter(execution, decision, &additional.play)?;
            } else {
                self.internal_material_parameter(execution, decision, false)?;
            }
        } else {
            self.internal_material_parameter(execution, decision, play.material_posture(decision))?;
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_play_parameters(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        let Some(mut play) = execution.play.take() else {
            return Ok(());
        };
        if self.internal_optional_decision(execution, &play)?
            || self.internal_resource_entry_question(execution, &play)?
            || self.internal_exchange_decision(execution, &play)?
            || self.internal_play_modes(execution, &mut play)?
        {
            execution.play = Some(play);
            return Ok(());
        }
        play.freeze_selection(self, execution)?;
        if self.internal_optional_decision(execution, &play)? {
            execution.play = Some(play);
            return Ok(());
        }
        while let Some(decision) = play.parameters.pop_front() {
            let snapshot = play
                .snapshot
                .as_ref()
                .ok_or_else(|| invalid("missing selection snapshot"))?;
            match &decision.domain {
                ChoiceDomain::Recipients { .. } => {
                    snapshot.internal_recipient_target(execution, &decision)?;
                    if let Some((view, _)) = execution.waiting.as_mut() {
                        self.state.ir_epoch = self
                            .state
                            .ir_epoch
                            .checked_add(1)
                            .ok_or_else(|| invalid("IR epoch exhausted"))?;
                        view.token.epoch = self.state.ir_epoch;
                    }
                }
                ChoiceDomain::Objects { .. } => {
                    if super::material::is_parameter(&decision) {
                        if play.declined_material(&decision) {
                            continue;
                        }
                        snapshot.internal_play_material_parameter(execution, &decision, &play)?;
                    } else {
                        snapshot.internal_target(execution, &decision)?;
                    }
                    // Target construction is pure; the live authority alone allocates the token.
                    if let Some((view, _)) = execution.waiting.as_mut() {
                        self.state.ir_epoch = self
                            .state
                            .ir_epoch
                            .checked_add(1)
                            .ok_or_else(|| invalid("IR epoch exhausted"))?;
                        view.token.epoch = self.state.ir_epoch;
                    }
                }
                ChoiceDomain::Distribution {
                    recipients, total, ..
                } => {
                    let members = execution.recipients(snapshot, recipients)?;
                    let total = execution.natural(snapshot, total)?;
                    if !members.members().is_empty()
                        && u64::try_from(members.members().len()).map_err(invalid)? > total.0
                    {
                        Self::retry_play_targets(execution, &mut play)?;
                        execution.play = Some(play);
                        return self.internal_play_parameters(execution);
                    }
                    // Distribution needs the same frozen game and the authority's live token allocator.
                    self.internal_distribution_in(
                        execution,
                        &decision,
                        snapshot,
                        play.snapshot_id,
                    )?;
                    if execution.waiting.is_some() {
                        play.distribution_captures = Some(decision);
                    } else {
                        execution.captures(snapshot, &decision.captures)?;
                    }
                }
                _ => return Err(unsupported("play parameter")),
            }
            if execution.waiting.is_some() {
                execution.play = Some(play);
                return Ok(());
            }
        }
        if self.internal_exchange_materials(execution, &mut play)? {
            execution.play = Some(play);
            return Ok(());
        }
        if self.internal_play_hand_materials(execution, &play)? {
            execution.play = Some(play);
            return Ok(());
        }
        self.finish_play_payment(execution, play)
    }

    fn internal_additional(
        &self,
        execution: &Execution,
    ) -> Result<Option<Box<(FaceId, PlayCostPayment)>>> {
        let object = self.object(&execution.context.source)?;
        let card = self
            .card_runtime
            .definitions
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("unloaded card"))?;
        Ok(card.card().card_play_costs.iter().find_map(|face| {
            face.node
                .additional
                .as_ref()
                .map(|payment| Box::new((face.node.face.clone(), payment.node.clone())))
        }))
    }

    fn internal_additional_payment(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<()> {
        let Some((face, payment)) = play.additional.as_deref() else {
            return Ok(());
        };
        self.internal_card_payment(execution, &payment.play.costs, false)?;
        let CostSpec::Ordered { steps } = &payment.play.costs else {
            return Err(unsupported("additional costs"));
        };
        execution.receipts.insert(
            payment.receipt,
            InstructionOutcome::ExchangePayment(PaymentResult {
                complete: Bool(true),
                steps: steps
                    .iter()
                    .map(|step| RuntimeReceiptId {
                        execution: execution.id,
                        slot: step.instruction.receipt,
                    })
                    .collect(),
            }),
        );
        execution.card_payment = Some((face.clone(), payment.receipt));
        Ok(())
    }

    fn finish_play_payment(&mut self, execution: &mut Execution, play: PlayState) -> Result<()> {
        let consumed = if execution.spell || execution.field_entry {
            self.internal_matching_player_costs(execution)?
        } else {
            vec![]
        };
        let previous = self.card_runtime.active_source;
        self.card_runtime.active_source = execution.ability;
        let payment = if play.resource_entry.is_some() {
            self.internal_resource_entry_payment(execution, &play)
        } else if play.exchange.is_some() || execution.field_entry {
            self.internal_exchange_payment(execution, &play)
        } else {
            self.internal_card_payment(execution, &play.costs, execution.spell)
        };
        let payment =
            payment.and_then(|()| self.internal_card_payment(execution, &play.mode_costs, false));
        let payment = payment.and_then(|()| self.internal_additional_payment(execution, &play));
        let payment = payment.and_then(|()| self.internal_optional_payment(execution, &play));
        self.card_runtime.active_source = previous;
        payment?;
        self.internal_consume_player_costs(&consumed);
        if execution.field_entry {
            return Ok(());
        }
        self.commit_internal_play(execution, play)
    }

    fn commit_internal_play(&mut self, execution: &mut Execution, play: PlayState) -> Result<()> {
        let source = execution
            .ability
            .ok_or_else(|| invalid("missing formal source"))?;
        if execution.spell {
            self.bump(
                &format!("{}.cards_played", seat_name(source.controller)?),
                1,
            );
            let group = self.group();
            self.emit(
                json!({"kind":"プレイ","object":execution.context.source}),
                &execution.frame().cause,
                group,
            );
        }
        let turn = TurnId(self.state.schedule.current()?);
        if let Some(limit) = play.limit {
            self.card_runtime
                .consume_quota(source.instance, turn, limit)?;
        }
        for group in &play.groups {
            if let ChoiceDomain::Modes {
                exclude_used: Some(_),
                ..
            } = group.chosen.domain
            {
                for mode in play
                    .chosen
                    .get(&group.chosen.id)
                    .ok_or_else(|| invalid("missing modes"))?
                {
                    let fact = ModeUse {
                        execution: execution.id,
                        ability_instance: source.instance,
                        mode: *mode,
                        turn,
                    };
                    if !self.card_runtime.mode_uses.contains(&fact) {
                        self.card_runtime.mode_uses.push(fact);
                    }
                }
            }
        }
        if let Some(nested) = execution.nested.as_mut() {
            nested.committed = true;
        }
        execution.facts.push(if execution.spell {
            Fact::CardPlayed {
                execution: execution.id,
                object: source.holder,
                controller: source.controller,
            }
        } else {
            Fact::AbilityPlayed {
                execution: execution.id,
                source,
                qualification: play.qualification,
            }
        });
        if !execution.spell && play.qualification == AbilityQualification::UnionBurst {
            self.bump(
                &format!("{}.ub_activated", seat_name(source.controller)?),
                1,
            );
            self.state.ir_ub_history.push(UbHistoryFact {
                execution: execution.id.0,
                controller: source.controller.seat,
            });
        }
        if play.qualification == AbilityQualification::UnionBurst {
            self.internal_ub_activated_events(execution)?;
        }
        if execution.spell {
            let CostSpec::Ordered { steps } = &play.costs else {
                return Err(unsupported("card play costs"));
            };
            let receipt = steps
                .first()
                .ok_or_else(|| invalid("missing intrinsic payment"))?
                .instruction
                .receipt;
            self.internal_played_card_events(execution, receipt)?;
        }
        let paid = play
            .optional
            .iter()
            .filter(|payment| payment.accepted == Some(true))
            .map(|payment| payment.receipt)
            .collect();
        execution.prepend(selected_body(play.body, &play.chosen, &paid)?);
        Ok(())
    }

    fn internal_play_modes(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
    ) -> Result<bool> {
        while let Some(group) = play.modes.pop_front() {
            if let Some(condition) = &group.condition
                && !execution.boolean(self, condition)?.0
            {
                play.chosen.insert(group.chosen.id, Vec::new());
                continue;
            }
            let count = execution.natural(self, &group.count)?;
            let ChoiceDomain::Modes {
                modes,
                cardinality,
                exclude_used,
                ..
            } = &group.chosen.domain
            else {
                return Err(unsupported("mode domain"));
            };
            let count = Nat(count.0.min(u64::try_from(modes.len()).map_err(invalid)?));
            if count == Nat(0) {
                play.chosen.insert(group.chosen.id, Vec::new());
                continue;
            }
            let minimum = if cardinality.maximum.is_none() {
                1
            } else {
                cardinality.minimum.0
            };
            let source = execution
                .ability
                .ok_or_else(|| invalid("missing mode source"))?;
            let turn = TurnId(self.state.schedule.current()?);
            let mut legal = Vec::new();
            for mode in &group.modes {
                if self.internal_mode_affordable(execution, play, &mode.costs)?
                    && self.internal_mode_targets_feasible(execution, &mode.parameters)?
                    && (exclude_used.is_none()
                        || !self.card_runtime.mode_uses.iter().any(|used| {
                            used.ability_instance == source.instance
                                && used.turn == turn
                                && used.mode == mode.id
                        }))
                {
                    legal.push(mode.id);
                }
            }
            let affordable = self.internal_play_affordable(execution, play)?;
            if u64::try_from(legal.len()).map_err(invalid)? < minimum || !affordable {
                return Err(crate::EngineFailure::IllegalPlay(
                    "no legal mode and cost plan".into(),
                ));
            }
            let mut chosen = group.chosen;
            let ChoiceDomain::Modes {
                modes: legal_modes,
                cardinality: legal_bounds,
                ..
            } = &mut chosen.domain
            else {
                return Err(unsupported("mode domain"));
            };
            *legal_modes = legal;
            *legal_bounds = Cardinality {
                minimum: Nat(minimum),
                maximum: Some(count),
            };
            self.internal_wait(
                execution,
                chosen.id,
                execution.context.controller,
                InputDomain::Modes {
                    modes: legal_modes.clone(),
                    cardinality: *legal_bounds,
                    count,
                },
                Wait::Modes(ModeWait { chosen, count }),
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn internal_mode_affordable(
        &self,
        execution: &Execution,
        play: &PlayState,
        costs: &CostSpec,
    ) -> Result<bool> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("mode costs"));
        };
        if steps.is_empty() {
            return Ok(true);
        }
        if play.exchange.is_some() {
            return Err(unsupported("exchange with mode costs"));
        }
        let mut probe = self.clone();
        let mut execution = execution.clone();
        probe.card_runtime.active_source = execution.ability;
        let spell = execution.spell;
        let paid = probe.internal_card_payment(&mut execution, &play.costs, spell);
        let paid = paid.and_then(|()| probe.internal_card_payment(&mut execution, costs, false));
        match paid {
            Ok(()) => Ok(true),
            Err(crate::EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn internal_play_affordable(&self, execution: &Execution, play: &PlayState) -> Result<bool> {
        if play.exchange.is_some() {
            return self.internal_exchange_affordable(execution, play);
        }
        let mut payment_game = self.clone();
        payment_game.card_runtime.active_source = execution.ability;
        match payment_game.internal_card_payment(
            &mut execution.clone(),
            &play.costs,
            execution.spell,
        ) {
            Ok(()) => Ok(true),
            Err(crate::EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(in crate::game::internal) fn internal_play_distribution_captures(
        execution: &mut Execution,
        decision: DecisionId,
    ) -> Result<()> {
        if let Some(mut play) = execution.play.take() {
            if let Some(captured) = play.distribution_captures.take() {
                if captured.id != decision {
                    return Err(invalid("foreign distribution captures"));
                }
                execution.captures(
                    play.snapshot
                        .as_ref()
                        .ok_or_else(|| invalid("missing snapshot"))?,
                    &captured.captures,
                )?;
            }
            execution.play = Some(play);
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_target_answer(
        execution: &mut Execution,
        wait: &super::selection::TargetWait,
        objects: Vec<ObjectRef>,
    ) -> Result<()> {
        let play = execution
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing target play"))?;
        let snapshot = play
            .snapshot
            .clone()
            .ok_or_else(|| invalid("missing target snapshot"))?;
        execution.bind_targets(wait, objects);
        execution.captures(&snapshot, &wait.decision.captures)
    }

    fn retry_play_targets(execution: &mut Execution, play: &mut PlayState) -> Result<()> {
        let restart = play
            .restart
            .as_ref()
            .ok_or_else(|| invalid("missing target rollback"))?;
        // Feasibility was proven against this same snapshot before the first target question.
        execution.slots.clone_from(&restart.slots);
        execution.targets.clear();
        execution.allocations.clear();
        play.parameters.clone_from(&restart.parameters);
        play.distribution_captures = None;
        Ok(())
    }

    pub(in crate::game::internal) fn internal_spell_cleanup(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        if execution.spell {
            let holder = &execution.context.source;
            let current = self.object(holder)?;
            if current.zone == "resolution" && current.generation == execution.context.generation.0
            {
                // The mandatory Advance return cannot reuse an exhausted physical generation.
                let moves = if self.object_type(holder)? == "スペル・アドバンス" {
                    2
                } else {
                    1
                };
                current
                    .generation
                    .checked_add(moves)
                    .ok_or_else(|| invalid("Spell cleanup generation exhausted"))?;
                let departing_source = execution.source_before_move(self, from_ref(holder))?;
                self.move_objects(from_ref(holder), "cemetery", None, None, &execution.frame())?;
                execution.seal_source_departure(&departing_source);
            }
        }
        Ok(())
    }
}

fn activation(
    progress: super::super::Progress,
    source: Option<RuntimeAbilitySource>,
) -> Result<Activation> {
    let source = source.ok_or_else(|| invalid("missing formal source"))?;
    let execution = match progress.facts.last() {
        Some(Fact::AbilityResolved {
            execution,
            source: resolved,
        }) if *resolved == source => *execution,
        Some(Fact::CardResolved {
            execution,
            object,
            controller,
        }) if *object == source.holder && *controller == source.controller => *execution,
        _ => return Err(invalid("formal play did not complete")),
    };
    Ok(Activation {
        execution,
        source,
        progress,
        resume: ResumeStage::CheckTiming,
    })
}

fn selected_body(
    mut program: Program,
    choices: &BTreeMap<DecisionId, Vec<ModeId>>,
    paid: &BTreeSet<ReceiptId>,
) -> Result<Program> {
    let mut steps = Vec::new();
    for mut located in program.steps {
        match located.node {
            Node::Control {
                control:
                    Control::Pay {
                        at: PaymentTime::PlayParameter,
                        on_paid,
                        receipt,
                        ..
                    },
            } => {
                if paid.contains(&receipt) {
                    steps.extend(selected_body(on_paid, choices, paid)?.steps);
                }
            }

            Node::Control {
                control:
                    Control::Mode {
                        modes,
                        chosen,
                        choice_kind: ModeChoiceKind::ChoiceKeyword,
                        ..
                    },
            } => {
                let chosen = choices
                    .get(&chosen.id)
                    .ok_or_else(|| invalid("missing saved mode result"))?;
                for mode in modes {
                    if chosen.contains(&mode.id) {
                        steps.extend(selected_body(mode.body, choices, paid)?.steps);
                    }
                }
            }
            Node::Control {
                control: Control::Sequence { body },
            } => {
                located.node = Node::Control {
                    control: Control::Sequence {
                        body: selected_body(body, choices, paid)?,
                    },
                };
                steps.push(located);
            }
            Node::Control {
                control:
                    Control::If {
                        condition,
                        then_body,
                        else_body,
                    },
            } => {
                located.node = Node::Control {
                    control: Control::If {
                        condition,
                        then_body: selected_body(then_body, choices, paid)?,
                        else_body: else_body
                            .map(|body| selected_body(body, choices, paid))
                            .transpose()?,
                    },
                };
                steps.push(located);
            }
            _ => steps.push(located),
        }
    }
    program.steps = steps;
    Ok(program)
}

fn independent_selector(selector: &Selector) -> bool {
    match selector {
        Selector::Zone { .. } | Selector::Objects { .. } => true,
        Selector::Union { sets } => sets.iter().all(independent_selector),
        Selector::Difference { input, except }
            if matches!(except.as_ref(), Selector::Objects { objects }
                if objects == &[ObjectOperand::SourceHolder {}]) =>
        {
            independent_selector(input)
        }
        _ => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Field plays share the card payment continuation."
)]
impl Game {
    pub(super) fn prepare_field_play(&self, execution: &mut Execution) -> Result<()> {
        execution.field_entry = true;
        execution.play = Some(PlayState {
            qualification: AbilityQualification::Ordinary,
            additional: None,
            resource_entry: None,
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: VecDeque::new(),
            groups: vec![],
            chosen: BTreeMap::new(),
            parameters: VecDeque::new(),
            exchange: self.internal_exchange(execution)?,
            optional: vec![],
            costs: CostSpec::Ordered { steps: vec![] },
            mode_costs: CostSpec::Ordered { steps: vec![] },
            limit: None,
            body: Program {
                scope: ScopeId(0),
                steps: vec![],
            },
            restart: None,
            distribution_captures: None,
        });
        Ok(())
    }
}

fn independent_mode_recipients(candidates: &RecipientSet) -> bool {
    match candidates {
        RecipientSet::Leaders { players } => players.relative_to == (PlayerRef::Controller {}),
        RecipientSet::Objects { objects } => independent_selector(objects),
        RecipientSet::Union { sets } => sets.iter().all(independent_mode_recipients),
        _ => false,
    }
}

fn independent_mode_target(decision: &Decision) -> bool {
    use crate::ir::{Capture, CaptureTime, ObjectView, PlayerRead, Role};
    if let ChoiceDomain::Recipients { candidates, .. } = &decision.domain {
        return decision.captures.is_empty() && independent_mode_recipients(candidates);
    }
    let ChoiceDomain::Objects {
        binding,
        constraint,
        ..
    } = &decision.domain
    else {
        return false;
    };
    if !constraint
        .groups
        .iter()
        .all(|group| independent_selector(&group.selector))
    {
        return false;
    }
    decision.captures.iter().all(|capture| {
        constraint.cardinality==(Cardinality{minimum:Nat(1),maximum:Some(Nat(1))}) && matches!(capture,Capture::Player{at:CaptureTime::AtSelection{decision:at},value:Expr::Live(read),..} if *at==decision.id && matches!(read.as_ref(),PlayerRead::Role{object:ObjectView::Selected{selection},role:Role::Controller} if selection==binding))
    })
}
