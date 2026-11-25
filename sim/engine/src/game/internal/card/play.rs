use super::super::CostPaymentPurpose;
use super::super::{Answer, DecisionToken, DecisionView, DeclarationMode, InputDomain, Slot, Wait};
use super::{
    Ability, Activation, Context, Execution, ExecutionId, Expr, Fact, Game, Generation, Int,
    Located, Nat, Node, PlayerId, Result, ResumeStage, RuntimeAbilitySource, SourceSpan, Step,
    admission, int, invalid, seat_name, unsupported,
};
use crate::game::internal::processing::{
    discard_selection_round, disclose_played_source, enter_play_phase, record_target_declaration,
};
use crate::game::internal::repeated_moves;
use crate::game::internal::{InstructionOutcome, player_id};
use crate::game::internal::{PlayPhase, TargetDeclaration};
use crate::game::knowledge::PublishedKnowledge;
use crate::game::legal::subsets;
use crate::ir::flip_cost as flip_payment;
use crate::ir::material_sets;
use crate::ir::quick::{is_printed_quick, printed_activation};
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::x_pp as x_payment;
use crate::ir::{choice_rules, optional_payment, public_numbers};

use crate::ir::{
    AbilityCostPolicy, DiscardMode, FaceId, PaymentResult, PlayCostPayment, RuntimeReceiptId,
};
use crate::ir::{
    AbilityKind, AbilityQualification, Action, Binding, Bool, Cardinality, LimitSpec, ModeUse,
    ObjectId, ObjectOperand, ObjectRef, PaymentTime, PlaySpec, PlayerRef, ReceiptId, RecipientSet,
    SelectionSnapshot, Selector, TargetRoleAnswer, TurnId,
};
use crate::ir::{
    ChoiceDomain, Control, CostSpec, Decision, DecisionId, DecisionKind, ExecutionRef, InputMode,
    Mode, ModeChoiceKind, ModeDecision, ModeDecisionKind, ModeId, NumericChange, PlaySlot, Program,
    Resource, ResourceOperation, ScopeId, SnapshotId, Visibility, ZoneKind,
};

pub(super) mod ability_cost;
pub(super) mod cost_observation;
mod counter_budget;
mod x_pp;
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::sync::Arc;
use core::slice::from_ref;
use serde_json::{Value, json};

/// A formal play exposes a question or a fully committed result, never partial receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardPlay {
    /// Effect victory ended the game; the executed prefix has no resolved fact.
    GameEnded(super::super::Terminal),
    /// Answer through `answer_internal_card` using only this token and a payload.
    Awaiting(DecisionView),
    /// Complete printed Spell or ability, followed by explicit check timing.
    Resolved(Activation),
    /// A Spell without a spell ability has completed, followed by explicit check timing.
    ResolvedCard(super::super::Progress),
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
                .filter(|decision| {
                    matches!(
                        decision.kind,
                        DecisionKind::PlayParameter {
                            slot: PlaySlot::Targets,
                            ..
                        }
                    )
                })
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
    cost_policy: AbilityCostPolicy,
    qualification: AbilityQualification,
    quick_activation: bool,
    pub(in crate::game::internal) resource_entry: Option<super::resource_entry::ResourceEntryState>,
    snapshot: Option<Arc<Game>>,
    pub(in crate::game::internal) snapshot_id: SnapshotId,
    modes: VecDeque<PlayMode>,
    groups: Vec<PlayMode>,
    modes_prepared: bool,
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
    pub(super) fn card_play_payment_receipt(&self, execution: &Execution) -> Result<ReceiptId> {
        if execution.ability.is_none() {
            return Ok(ReceiptId(0));
        }
        let CostSpec::Ordered { steps } = &self.costs else {
            return Err(unsupported("intrinsic costs"));
        };
        steps
            .first()
            .map(|step| step.instruction.receipt)
            .ok_or_else(|| invalid("missing intrinsic payment"))
    }

    pub(in crate::game::internal) fn required_head_pending(&self) -> bool {
        self.optional
            .iter()
            .any(|payment| payment.required && payment.accepted.is_none())
    }
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
        self.material_for_payment(decision, |payment| payment.accepted == Some(false))
    }

    fn material_for_payment(
        &self,
        decision: &Decision,
        matches_payment: impl Fn(&super::optional::OptionalState) -> bool,
    ) -> bool {
        let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
            return false;
        };
        self.optional.iter().any(|payment| {
            matches_payment(payment)
                && matches!(&payment.costs, CostSpec::Ordered { steps } if steps.iter().any(|step| {
                    let action = &step.instruction.action;
                    matches!(super::material::cost_objects(action).or_else(|| reveal_payment::objects(action)), Some(Selector::Bound { reference }) if reference == binding)
                        || super::super::counters::selected_cost(action) == Some(*binding)
                }))
        })
    }

    fn prepare_modes(&mut self) -> Result<()> {
        if !self.modes_prepared {
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
                order_parameters(self.parameters.make_contiguous());
            }
            self.modes_prepared = true;
        }
        Ok(())
    }

    fn freeze_selection(&mut self, game: &Game, execution: &Execution) -> Result<()> {
        if self.snapshot.is_none()
            && game.has_target_cost(execution, None)?
            && !game.internal_play_affordable(execution, self)?
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "no affordable formal target plan".into(),
            ));
        }
        if self.snapshot.is_none() && !self.parameters.is_empty() {
            // The first target fixes the game view; later answers only extend immutable slots.
            self.snapshot_id = SnapshotId(game.state.ir_epoch);
            self.snapshot = Some(Arc::new(game.clone()));
            // Never ask a target question that no target and distribution plan can satisfy.
            if self
                .parameters
                .iter()
                .any(|decision| matches!(decision.domain, ChoiceDomain::Distribution { .. }))
            {
                let proof = bound_distribution_reveal(game, execution, &mut self.parameters)?;
                if !game.internal_feasible_targets(execution, &execution.slots, &proof)? {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "no legal target and distribution plan".into(),
                    ));
                }
            }
            self.restart = Some(SelectionOrigin {
                slots: execution.slots.clone(),
                parameters: self.parameters.clone(),
            });
        }
        Ok(())
    }
}

// A variable reveal is paid after targets. Its available cards bound the target question,
// while only the eventual reveal answer determines the distribution total.
fn bound_distribution_reveal(
    game: &Game,
    execution: &Execution,
    parameters: &mut VecDeque<Decision>,
) -> Result<VecDeque<Decision>> {
    let mut proof = parameters.clone();
    for distribution in &mut proof {
        let ChoiceDomain::Distribution {
            recipients:
                RecipientSet::Objects {
                    objects: Selector::Bound { reference },
                },
            total,
            minimum: Nat(1),
            ..
        } = &mut distribution.domain
        else {
            continue;
        };
        let Some(material) = public_numbers::material_count(total) else {
            continue;
        };
        let Some(reveal_index) = parameters.iter().position(|decision| {
            reveal_payment::parameter(decision)
                .is_some_and(|(binding, bounds)| binding == material && bounds.maximum.is_none())
        }) else {
            continue;
        };
        let reveal = parameters
            .remove(reveal_index)
            .ok_or_else(|| invalid("missing distribution reveal"))?;
        let ChoiceDomain::Objects {
            constraint: reveal_constraint,
            ..
        } = &reveal.domain
        else {
            return Err(invalid("distribution reveal domain"));
        };
        let selector = &reveal_constraint
            .groups
            .first()
            .ok_or_else(|| invalid("distribution reveal selector"))?
            .selector;
        let available = u64::try_from(game.material_candidates(execution, selector, false)?.len())
            .map_err(invalid)?;

        let target = parameters
            .iter_mut()
            .find_map(|decision| match &mut decision.domain {
                ChoiceDomain::Objects {
                    constraint,
                    binding,
                    ..
                } if binding == reference => Some(constraint),
                _ => None,
            })
            .ok_or_else(|| invalid("missing distribution targets"))?;
        target.cardinality.maximum = Some(Nat(target
            .cardinality
            .maximum
            .map_or(available, |maximum| maximum.0.min(available))));

        let index = parameters
            .iter()
            .position(|decision| decision.id == distribution.id)
            .ok_or_else(|| invalid("missing reveal distribution"))?;
        parameters.insert(index, reveal);
        *total = Expr::Literal(Nat(available));
    }
    Ok(proof)
}

fn distribution_reveal_minimum(
    game: &Game,
    execution: &Execution,
    parameters: &VecDeque<Decision>,
    material: Binding<Selector>,
) -> Result<Nat> {
    let mut required = 0;
    for decision in parameters {
        if let ChoiceDomain::Distribution {
            recipients,
            total,
            minimum: Nat(1),
            ..
        } = &decision.domain
            && public_numbers::material_count(total) == Some(material)
        {
            let count = u64::try_from(execution.recipients(game, recipients)?.members().len())
                .map_err(invalid)?;
            required = required.max(count);
        }
    }
    Ok(Nat(required))
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
        || optional_payment::declaration(body).is_some_and(|(costs, _)| !x_payment::payment(costs))
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
                    operation: ResourceOperation::Pay, amount } if matches!(amount, Expr::Literal(Int(amount)) if *amount >= 0)
                        || x_payment::amount(amount).is_some_and(|binding| mode.parameters.iter()
                            .any(|parameter| x_payment::parameter(parameter) == Some(binding))))) {
                return Err(unsupported("mode PP payment"));
            }
            check_private_mode_cost(ability, mode)?;
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
    if parameters
        .iter()
        .any(|decision| x_payment::parameter(decision).is_some())
        && is_printed_quick(ability)
    {
        return Err(unsupported("X with Quick"));
    }
    check_overlapping_mode_materials(play, &groups)?;
    order_parameters(&mut parameters);
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
        ability.kind != AbilityKind::Spell,
    )
}

/// The eligible sets of every chosen discard among these cost steps.
fn chosen_discards(costs: &CostSpec) -> Vec<Vec<&Selector>> {
    let CostSpec::Ordered { steps } = costs else {
        return Vec::new();
    };
    steps
        .iter()
        .filter_map(|step| match &step.instruction.action {
            Action::Discard {
                mode: DiscardMode::Chosen { constraint },
                ..
            } => Some(
                constraint
                    .groups
                    .iter()
                    .map(|group| &group.selector)
                    .collect(),
            ),
            _ => None,
        })
        .collect()
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
    // Choices that can never name the same card pay through the ordinary per-binding flow.
    // Chosen hand discards are cost steps, not parameters; their eligible sets join the proof.
    let declarations = groups
        .iter()
        .flat_map(|group| &group.modes)
        .filter_map(|mode| {
            optional_payment::mode_declaration(&mode.body).map(|(costs, _, _)| costs)
        })
        .collect::<Vec<_>>();
    let mut sets = chosen_discards(&play.costs);
    sets.extend(declarations.iter().flat_map(|costs| chosen_discards(costs)));
    let readable = play
        .parameters
        .iter()
        .chain(
            groups
                .iter()
                .flat_map(|group| &group.modes)
                .flat_map(|mode| &mode.parameters),
        )
        .filter(|decision| super::material::is_parameter(decision))
        .map(material_sets::choice_selectors)
        .collect::<Option<Vec<_>>>();
    if let Some(parameters) = readable {
        sets.extend(parameters);
        if material_sets::disjoint_sets(&sets) {
            return Ok(());
        }
    }
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

fn check_private_mode_cost(ability: &Ability, mode: &Mode) -> Result<()> {
    if mode
        .parameters
        .iter()
        .any(|decision| reveal_payment::parameter(decision).is_some())
        && !reveal_payment::field_mode(ability, mode)
    {
        return Err(unsupported("private hand mode cost entry"));
    }
    Ok(())
}

fn material_dependency(decision: &Decision) -> Option<Binding<Selector>> {
    match &decision.domain {
        ChoiceDomain::Objects { constraint, .. } => constraint
            .aggregate_bounds
            .iter()
            .find_map(|bound| public_numbers::material_target_limit(&bound.value)),
        ChoiceDomain::Distribution { total, .. } => public_numbers::material_count(total),
        _ => None,
    }
}

fn material_dependencies(parameters: &[Decision]) -> BTreeSet<(ScopeId, u64)> {
    parameters
        .iter()
        .filter_map(material_dependency)
        .map(Binding::key)
        .collect()
}

fn order_parameters(parameters: &mut [Decision]) {
    let dependencies = material_dependencies(parameters);
    parameters.sort_by_key(|decision| {
        if x_payment::parameter(decision).is_some() {
            0_u8
        } else if super::material::is_parameter(decision) {
            if matches!(&decision.domain, ChoiceDomain::Objects { binding, .. }
                if dependencies.contains(&binding.key()))
            {
                1_u8
            } else {
                3_u8
            }
        } else {
            2_u8
        }
    });
}

fn check_parameters(parameters: &[Decision]) -> Result<()> {
    let mut distribution = false;
    let mut late_material = false;
    let dependencies = material_dependencies(parameters);
    let mut public_materials = BTreeSet::new();
    let mut private_reveals = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let mut declarations = true;
    for decision in parameters {
        if x_payment::parameter(decision).is_some() {
            if !declarations {
                return Err(unsupported("X must precede targets and materials"));
            }
            continue;
        }
        declarations = false;
        if super::material::is_parameter(decision) {
            super::material::check_parameter(decision)?;
            if decision.visibility == (Visibility::Public {})
                && let ChoiceDomain::Objects { binding, .. } = &decision.domain
            {
                public_materials.insert(binding.key());
                late_material |= !dependencies.contains(&binding.key());
            } else if let Some((binding, _)) = reveal_payment::parameter(decision)
                && dependencies.contains(&binding.key())
            {
                private_reveals.insert(binding.key());
            } else {
                late_material = true;
            }
            continue;
        }
        if late_material
            || material_dependency(decision).is_some_and(|binding| {
                !(public_materials.contains(&binding.key())
                    || (matches!(decision.domain, ChoiceDomain::Distribution { .. })
                        && private_reveals.contains(&binding.key())))
            })
        {
            return Err(unsupported("target and material declaration order"));
        }
        check_target_parameter(decision, &mut targets, &mut distribution)?;
    }
    Ok(())
}

fn check_target_parameter(
    decision: &Decision,
    targets: &mut BTreeSet<(ScopeId, u64)>,
    distribution: &mut bool,
) -> Result<()> {
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
                target_roles,
            },
        ) if !*distribution => {
            super::selection::check(constraint)?;
            if target_roles.is_some() {
                for group in &constraint.groups {
                    super::selection::field_selector(&group.selector)?;
                }
            } else {
                targets.insert(binding.key());
            }
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
        ) if targets.contains(&reference.key()) => {
            *distribution = true;
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
        ) if !*distribution => {
            super::recipient_targets::check(candidates, *cardinality)?;
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
    Ok(())
}

fn spell_origin(zone: &str) -> Result<ZoneKind> {
    match zone {
        "hand" => Ok(ZoneKind::Hand),
        "ex" => Ok(ZoneKind::Ex),
        "banish" => Ok(ZoneKind::Banish),
        _ => Err(unsupported("Spell play origin")),
    }
}

const fn mode_minimum(bounds: Cardinality) -> u64 {
    if bounds.maximum.is_none() {
        1
    } else {
        bounds.minimum.0
    }
}

fn conditional_play_modes(ability: &Ability) -> bool {
    ability.play.is_some()
        && ability
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
    let public_condition = optional_payment::public_mode_guard(condition, then_body);
    if !allowed || groups.len() != 1 || !other.is_empty() || !public_condition {
        return Err(unsupported("conditional play mode"));
    }
    for group in &mut groups {
        group.condition = Some(condition.clone());
    }
    Ok(groups)
}

fn quick_cost_shape(action: &Action) -> bool {
    match action {
        Action::Resource {
            amount: Expr::Literal(_),
            ..
        }
        | Action::Counter {
            amount: Expr::Literal(_),
            ..
        }
        | Action::Posture { .. }
        | Action::Life {
            change: NumericChange::Add {
                amount: Expr::Literal(_),
            },
            ..
        }
        | Action::Move { .. } => true,
        Action::Discard { .. } => super::hand_payment::source_discard(action),
        Action::Banish {
            objects: Selector::Objects { objects },
        } => objects == &[ObjectOperand::SourceHolder {}],
        _ => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal plays share the authority's execution and rollback boundary."
)]
impl Game {
    fn internal_choice_any(&self, actor: PlayerId) -> Result<bool> {
        for holder in self.zone_ids(seat_name(actor)?, "ex") {
            let (card, _) = self.internal_effective_print(&holder)?;
            if self.card_runtime.is_loaded(&card)
                && self
                    .internal_printed_abilities(&holder)?
                    .iter()
                    .any(|ability| choice_rules::is_static(&ability.node))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(in crate::game::internal) fn internal_quick_preflight(
        &self,
        holder: &str,
        ability: &Ability,
        actor: PlayerId,
        spell: bool,
    ) -> Result<bool> {
        if !spell && self.internal_ability_forbidden(holder, ability)? {
            return Ok(false);
        }
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
        if spell {
            execution.play_origin = Some(spell_origin(&self.object(holder)?.zone)?);
        }
        let material = play.parameters.iter().any(super::material::is_parameter);
        if material
            || (!spell && play.limit.is_some())
            || super::qualification(&play.costs)?.is_some()
        {
            let source = probe.internal_card_source(holder, ability.id, actor)?;
            execution.bind_source(&probe, source)?;
            probe.card_runtime.active_source = Some(source);
            if !spell
                && let Some(limit) = play.limit
                && !probe.card_runtime.quota_available(
                    source.instance,
                    TurnId(probe.state.schedule.current()?),
                    limit,
                )
            {
                return Ok(false);
            }
            match probe.internal_use_prerequisite(&execution, &play.costs) {
                Ok(()) => {}
                Err(crate::EngineFailure::IllegalPlay(_)) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        execution.captures(&probe, &play.captures)?;
        let available = if spell && probe.has_target_cost(&execution, None)? {
            let mut payment_game = self.clone();
            let payment = payment_game.spell_execution(holder, actor, ability)?;
            let parameters = play.parameters.iter().cloned().collect();
            let mut affordable = false;
            for mut trial in
                payment_game.internal_public_cost_probes(&payment, &parameters, |decision| {
                    super::material::posture_parameter(&play.costs, decision)
                })?
            {
                let mut game = payment_game.clone();
                game.card_runtime.active_source = trial.ability;
                if game.internal_payment_probe(&mut trial, &play.costs, true)? {
                    affordable = true;
                    break;
                }
            }
            affordable
        } else if material {
            probe.internal_quick_material_costs_available(&execution, play)?
        } else {
            self.quick_costs_available(holder, ability, &execution, actor, spell)?
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
            conditional_play_modes(ability),
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
        ability: &Ability,
        execution: &Execution,
        actor: PlayerId,
        spell: bool,
    ) -> Result<bool> {
        let costs = &ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Quick PlaySpec"))?
            .costs;
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("Quick costs"));
        };
        // Keep the existing quote language while replacing its payment proof.
        if !steps
            .iter()
            .all(|step| quick_cost_shape(&step.instruction.action))
        {
            return Err(unsupported("Quick cost proof"));
        }
        // Keep the old Quick ownership boundary until departure costs are widened separately.
        if steps
            .iter()
            .any(|step| matches!(step.instruction.action, Action::Banish { .. }))
            && self.object(holder)?.owner != seat_name(actor)?
        {
            return Ok(false);
        }
        let mut probe = self.clone();
        let mut payment = if spell {
            probe.spell_execution(holder, actor, ability)?
        } else {
            let mut payment = execution.clone();
            let source = probe.internal_card_source(holder, ability.id, actor)?;
            payment.bind_source(&probe, source)?;
            payment
        };
        probe.card_runtime.active_source = payment.ability;
        let quoted = if spell {
            Cow::Borrowed(costs)
        } else {
            probe.internal_quick_activation_payment(&payment, costs)?.0
        };
        probe.internal_payment_probe(&mut payment, &quoted, spell)
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
        if !payment.play.parameters.is_empty()
            && payment
                .play
                .parameters
                .iter()
                .all(|decision| flip_payment::material(decision).is_some())
        {
            let mut probe = self.clone();
            let mut selection = probe.spell_execution(holder, actor, ability)?;
            probe.card_runtime.active_source = selection.ability;
            let play = ability
                .play
                .as_ref()
                .ok_or_else(|| invalid("missing Quick play"))?;
            if !probe.internal_payment_probe(&mut selection, &play.costs, true)? {
                return Ok(false);
            }
            return probe.flip_payment_available(
                &selection,
                &payment.play,
                &payment.play.parameters,
            );
        }
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
        if !probe.internal_payment_probe(&mut selection, &play.costs, true)? {
            return Ok(false);
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
            wait.record(&mut paid, members);
            if trial.internal_payment_probe(&mut paid, &payment.play.costs, false)? {
                return Ok(true);
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
        if let Some(condition) = &group.condition
            && !execution.boolean(self, condition)?.0
        {
            return self.quick_mode_plan(execution, rest, parameters);
        }
        let mut count = execution.natural(self, &group.count)?;
        let mut chosen = group.chosen.clone();
        let ChoiceDomain::Modes { cardinality, .. } = &mut chosen.domain else {
            return Err(unsupported("Quick mode domain"));
        };
        if count == Nat(0) {
            return self.quick_mode_plan(execution, rest, parameters);
        }
        if mode_minimum(*cardinality) > 0
            && self.internal_choice_any(execution.context.controller)?
        {
            count = Nat(u64::try_from(group.modes.len()).map_err(invalid)?);
            // R-0015: "好きな数" widens the maximum only; a choice still takes at least one mode.
            *cardinality = Cardinality {
                minimum: Nat(1),
                maximum: Some(count),
            };
        }
        let cardinality = *cardinality;
        let names = group
            .modes
            .iter()
            .enumerate()
            .map(|(i, _)| i.to_string())
            .collect::<Vec<_>>();
        let maximum = cardinality.maximum.map_or(count.0, |n| n.0.min(count.0));
        for selection in subsets(
            &names,
            usize::try_from(if cardinality.minimum == Nat(0) {
                0
            } else {
                mode_minimum(cardinality)
            })
            .map_err(invalid)?,
            usize::try_from(maximum).map_err(invalid)?,
        ) {
            let selected = group
                .modes
                .iter()
                .enumerate()
                .filter(|(index, _)| selection.contains(&index.to_string()))
                .map(|(_, mode)| mode.id)
                .collect::<Vec<_>>();
            if !chosen.accepts_answer(count, &selected) {
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
        self.play_main_card(holder, actor, &mut false)
    }

    pub(in crate::game) fn play_main_card(
        &mut self,
        holder: &str,
        actor: PlayerId,
        declared: &mut bool,
    ) -> Result<CardPlay> {
        self.require_internal_abilities(holder, "card play")?;
        if self.card_runtime.quick.is_some() {
            return Err(invalid("Quick requires its authority question"));
        }
        let before = self.clone();
        let published = before.published_knowledge();
        let result = self.run_card_declaration(holder, actor, &published, declared);
        self.finish_card_attempt(before, result)
    }

    pub(super) fn run_card_declaration(
        &mut self,
        holder: &str,
        actor: PlayerId,
        published: &PublishedKnowledge,
        declared: &mut bool,
    ) -> Result<CardPlay> {
        let mut execution = self.prepare_card_execution(holder, actor, None)?;
        *declared = true;
        self.internal_run(&mut execution, published)?;
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
        execution.play_origin = Some(spell_origin(&object.zone)?);
        execution.play_origin_object = Some(execution.reference(self, holder)?);
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

    pub(super) fn check_card_play_context(&self, holder: &str, actor: PlayerId) -> Result<()> {
        self.internal_context(&Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller: actor,
        })
    }

    pub(super) fn card_play_permission(&self, holder: &str, actor: PlayerId) -> Result<bool> {
        let object = self.object(holder)?;
        let seat = seat_name(actor)?;
        if (object.owner != seat && object.zone != "ex")
            || object.controller != seat
            || !matches!(object.zone.as_str(), "hand" | "ex" | "cemetery" | "banish")
            || int(&object.state["face"]) != 0
            || !self.prepared.is_empty()
            || object.state["evolved_with"].is_string()
        {
            return Ok(false);
        }
        if (object.zone == "cemetery"
            && !self.internal_card_cemetery_play_allowed(holder, actor)?)
            || (object.zone == "banish"
                && !self.internal_card_banish_play_allowed(holder, actor)?)
            || !self.internal_play_timing(actor, self.internal_quick_spell(holder)?)
            || self.restricted(holder, "play")?
            || self.internal_card_play_forbidden(holder, actor)?
        {
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn prepare_activated_execution(
        &mut self,
        ability: admission::PlannedAbility,
        context: Context,
        source: RuntimeAbilitySource,
        spell: bool,
    ) -> Result<Execution> {
        let mut execution = self.new_card_execution(context)?;
        execution.bind_source(self, source)?;
        execution.spell = spell;
        if let Some(play) = &ability.node.play {
            self.internal_use_prerequisite(&execution, &play.costs)?;
            execution.captures(self, &play.captures)?;
        }
        self.prepare_ability_play(&mut execution, ability, AbilityCostPolicy::Normal)?;
        Ok(execution)
    }

    fn internal_play_counts(&self) -> Result<BTreeMap<u32, (Nat, Nat)>> {
        let mut counts = BTreeMap::new();
        for seat in [1, 2] {
            let cards = self.zone_ids(seat_name(PlayerId { seat })?, "cemetery");
            let spells = self.internal_spell_chain(PlayerId { seat })?;
            counts.insert(
                seat,
                (spells, Nat(u64::try_from(cards.len()).map_err(invalid)?)),
            );
        }
        Ok(counts)
    }

    pub(super) fn new_card_execution(&mut self, context: Context) -> Result<Execution> {
        self.emitted.clear();
        let execution = self.fresh_card_execution(context)?;
        #[cfg(feature = "runner")]
        self.referee_trace_execution(execution.id.0, None);
        Ok(execution)
    }

    pub(in crate::game::internal) fn fresh_card_execution(
        &mut self,
        context: Context,
    ) -> Result<Execution> {
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
        let mut execution = self.provider_card_execution(context, ids);
        execution.play_counts = self.internal_play_counts()?;
        Ok(execution)
    }

    pub(in crate::game::internal) fn provider_card_execution(
        &self,
        context: Context,
        ids: BTreeMap<ObjectId, String>,
    ) -> Execution {
        let play_origin =
            self.state
                .objects
                .get(&context.source)
                .and_then(|object| match object.zone.as_str() {
                    "hand" => Some(ZoneKind::Hand),
                    "ex" => Some(ZoneKind::Ex),
                    _ => None,
                });
        Execution {
            plan: None,
            terminated: false,
            context,
            id: ExecutionId(self.state.ir_epoch),
            timestamp: self.state.ir_play_clock,
            ids,
            todo: VecDeque::new(),
            slots: BTreeMap::new(),
            procedures: Vec::new(),
            repeats: repeated_moves::State::default(),
            receipts: BTreeMap::new(),
            cost_materials: BTreeMap::new(),
            hand_cost_groups: BTreeMap::new(),
            fusion_material_pending: vec![],
            movement_capacity: BTreeMap::new(),
            material_selections: BTreeMap::new(),
            effect_evolution_destinations: BTreeMap::new(),
            facts: super::super::fact_log::Facts::default(),
            waiting: None,
            question_stage: None,
            play_phase: None,
            processing_disclosure: super::super::processing::ProcessingDisclosure::default(),
            number_declarations: Vec::new(),
            play_declarations: Vec::new(),
            pp_payments: Vec::new(),
            cost_spec_payments: Vec::new(),
            exchange_payments: Vec::new(),
            target_declarations: Vec::new(),
            destroy_guards: Vec::new(),
            look_viewers: BTreeMap::new(),
            player_receipt_links: BTreeMap::new(),
            reveals: Vec::new(),
            allocations: BTreeMap::new(),
            origin: None,
            ability: None,
            source_information: None,
            event_reads: super::super::event_reads::EventReads::default(),
            targets: BTreeMap::new(),
            selected_target_names: BTreeMap::new(),
            play: None,
            spell: false,
            field_entry: false,
            declaration_mode: DeclarationMode::Live,
            play_origin,
            play_origin_object: None,
            card_payment: None,
            trigger_occurrence: None,
            play_counts: BTreeMap::new(),
            ward_entries: Vec::new(),
            creating: None,
            nested: None,
            cost_override: None,
            play_origin_player: None,
            began: self.player_event_serial(),
        }
    }

    pub(super) fn run_card_play(
        &mut self,
        mut execution: Execution,
        ability: admission::PlannedAbility,
        published: &PublishedKnowledge,
    ) -> Result<CardPlay> {
        let resolution_cost = super::optional::targeted_discard_pp_cost(&ability.node).cloned();
        self.prepare_ability_play(&mut execution, ability, AbilityCostPolicy::Normal)?;
        if !self.pending_declaration_payable(&execution, resolution_cost.as_ref())? {
            return Err(crate::EngineFailure::IllegalPlay(
                "unplayable pending ability".into(),
            ));
        }
        self.internal_run(&mut execution, published)?;
        self.card_play_result(execution)
    }

    fn pending_declaration_payable(
        &self,
        execution: &Execution,
        resolution_cost: Option<&CostSpec>,
    ) -> Result<bool> {
        let probe = self.declaration_probe();
        // The discard ability's formal target is used only by its paid branch.
        if let Some(costs) = resolution_cost
            && !probe
                .clone()
                .internal_payment_probe(&mut execution.clone(), costs, false)?
        {
            return Ok(false);
        }
        probe.declaration_may_be_payable(Ok(execution.clone()))
    }

    pub(super) fn prepare_ability_play(
        &self,
        execution: &mut Execution,
        ability: admission::PlannedAbility,
        cost_policy: AbilityCostPolicy,
    ) -> Result<()> {
        let (ability, body_plan) = ability.into_parts();
        execution.plan = Some(body_plan);
        self.internal_require_ability_permission(&execution.context.source, &ability)?;
        let conditional = conditional_play_modes(&ability);
        let resource_entry = super::resource_entry::payment(&ability)?;
        let quick_activation = printed_activation(&ability);
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
        let declared_parameters = additional
            .as_deref()
            .into_iter()
            .flat_map(|(_, payment)| payment.play.parameters.clone())
            .chain(if resource_entry.is_some() {
                vec![]
            } else {
                play.parameters
            });
        // CR 10.6.2.2.4 fixes X before determining additional hand materials.
        let (mut parameters, following): (VecDeque<_>, VecDeque<_>) =
            declared_parameters.partition(|decision| x_payment::parameter(decision).is_some());
        parameters.extend(following);
        execution.play = Some(PlayState {
            cost_policy,
            qualification: ability.qualification,
            quick_activation,
            additional,
            resource_entry,
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: groups.clone().into(),
            groups,
            modes_prepared: false,
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
        enter_play_phase(execution, PlayPhase::Declare)?;
        super::super::processing::disclose_declared_source(self, execution);
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

    pub(super) fn card_play_result(&mut self, mut execution: Execution) -> Result<CardPlay> {
        if execution.terminated {
            return Ok(CardPlay::GameEnded(
                self.internal_effect_terminal(execution.id, execution.progress()),
            ));
        }
        super::super::processing::disclose_source(self, &mut execution);
        if let Some((view, _)) = &execution.waiting {
            let view = view.clone();
            self.typed = Some(execution);
            Ok(CardPlay::Awaiting(view))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            resolved_play(execution.progress(), execution.ability, execution.spell)
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
        self.answer_internal_card_validated(actor, token, answer, &mut false)
    }

    pub(in crate::game::internal) fn answer_internal_card_validated(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
        validated: &mut bool,
    ) -> Result<CardPlay> {
        if self.card_runtime.quick.is_some() {
            return self.answer_internal_quick(actor, token, &answer, validated);
        }
        if self.typed.is_none() && self.card_runtime.question.is_some() {
            return self.answer_internal_pending(actor, token, &answer, validated);
        }
        if self.typed.as_ref().is_none_or(|execution| {
            execution.ability.is_none() && !execution.field_entry && !execution.spell
        }) {
            return Err(invalid("no pending formal play"));
        }
        if self.decline_internal_head_payment(actor, token, &answer)? {
            *validated = true;
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
        let spell = self.typed.as_ref().is_some_and(|execution| execution.spell);
        let progress = match self.answer_internal_step(actor, token, answer, validated) {
            Err(crate::EngineFailure::IllegalPlay(_)) if occurrence.is_some() => {
                let (id, checkpoint) = occurrence.ok_or_else(|| invalid("missing occurrence"))?;
                self.internal_cancel_occurrence(id, checkpoint);
                return Ok(CardPlay::Cancelled);
            }
            result => result?,
        };
        if self.state.game["ended"] == true && !progress.complete {
            return Ok(CardPlay::GameEnded(
                self.internal_effect_terminal(token.execution, progress),
            ));
        }
        if let Some((view, _)) = self
            .typed
            .as_ref()
            .and_then(|execution| execution.waiting.as_ref())
        {
            Ok(CardPlay::Awaiting(view.clone()))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            resolved_play(progress, source, spell)
        }
    }

    fn internal_play_material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        play: &PlayState,
    ) -> Result<()> {
        if let Some((binding, bounds)) = reveal_payment::parameter(decision) {
            if let Some((_, additional)) = play.additional.as_deref() {
                self.internal_reveal_material_parameter(execution, decision, &additional.play)?;
            } else {
                let minimum = if bounds.maximum.is_none() {
                    distribution_reveal_minimum(self, execution, &play.parameters, binding)?
                } else {
                    Nat(0)
                };
                self.internal_material_parameter_with_minimum(execution, decision, false, minimum)?;
            }
        } else {
            self.internal_material_parameter(execution, decision, play.material_posture(decision))?;
        }
        let Some((_, Wait::MaterialParameter(wait))) = execution.waiting.as_mut() else {
            return Err(invalid("formal material producer lost its wait"));
        };
        // Shared material constructors also serve probes and inline evolution without a declaration.
        wait.mark_formal_parameter(decision.id);
        Ok(())
    }

    fn stamp_play_selection(&mut self, execution: &mut Execution) -> Result<()> {
        if let Some((view, _)) = execution.waiting.as_mut() {
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("IR epoch exhausted"))?;
            view.token.epoch = self.state.ir_epoch;
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
        play.waive_original_costs(execution)?;
        if self.internal_optional_decision(execution, &mut play)?
            || self.internal_resource_entry_question(execution, &play)?
            || self.internal_exchange_decision(execution, &play)?
            || self.internal_play_modes(execution, &mut play)?
        {
            execution.play = Some(play);
            return Ok(());
        }
        play.waive_original_costs(execution)?;
        play.prepare_modes()?;
        if self.internal_x_parameter(execution, &mut play)? {
            execution.play = Some(play);
            return Ok(());
        }
        play.freeze_selection(self, execution)?;
        if self.internal_optional_decision(execution, &mut play)? {
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
                    self.stamp_play_selection(execution)?;
                }
                ChoiceDomain::Distribution {
                    recipients, total, ..
                } => {
                    if !super::super::allocation::feasible(execution, snapshot, recipients, total)?
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
                        Some(decision.id),
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
        self.internal_card_payment(
            execution,
            &payment.play.costs,
            false,
            Some(CostPaymentPurpose::Additional {
                receipt: payment.receipt,
            }),
        )?;
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
        let targets = self.internal_targeted_occurrences(execution)?;
        enter_play_phase(execution, PlayPhase::Pay)?;
        let (quoted, consumed) = if play.quick_activation {
            self.internal_quick_activation_payment(execution, &play.costs)?
        } else if execution.spell || execution.field_entry {
            (
                Cow::Borrowed(&play.costs),
                self.internal_matching_player_costs(execution)?,
            )
        } else {
            (Cow::Borrowed(&play.costs), vec![])
        };
        let previous = self.card_runtime.active_source;
        self.card_runtime.active_source = execution.ability;
        let payment = if play.resource_entry.is_some() {
            self.internal_resource_entry_payment(execution, &play)
        } else if play.exchange.is_some() || execution.ability.is_none() {
            self.internal_exchange_payment(execution, &play)
        } else {
            self.internal_card_payment(
                execution,
                &quoted,
                execution.spell,
                Some(CostPaymentPurpose::Original),
            )
        };
        let payment = payment.and_then(|()| {
            self.internal_card_payment(
                execution,
                &play.mode_costs,
                false,
                Some(CostPaymentPurpose::Modes),
            )
        });
        let payment = payment.and_then(|()| self.internal_additional_payment(execution, &play));
        let payment = payment.and_then(|()| self.internal_optional_payment(execution, &play));
        self.card_runtime.active_source = previous;
        payment?;
        if execution.probing_declaration() {
            return if execution.field_entry {
                self.check_field_play_capacity(execution.context.controller)
            } else {
                Ok(())
            };
        }
        self.internal_consume_player_costs(&consumed);
        self.internal_commit_targeted(targets)?;
        if execution.field_entry {
            return Ok(());
        }
        self.commit_internal_play(execution, play)
    }

    fn commit_internal_play(&mut self, execution: &mut Execution, play: PlayState) -> Result<()> {
        let controller = execution
            .ability
            .map_or(execution.context.controller, |source| source.controller);
        if execution.spell {
            self.internal_commit_card_play(&execution.context.source, seat_name(controller)?)?;
            let group = self.group();
            let player_step = self.emit(
                json!({"kind":"プレイ","object":execution.context.source}),
                &execution.frame().cause,
                group,
            );
            self.player_step_actor(&player_step, &execution.frame().controller);
        }
        if let Some(source) = execution.ability {
            self.commit_ability_use(execution, &play, source)?;
        }
        if let Some(nested) = execution.nested.as_mut() {
            nested.committed = true;
        }
        enter_play_phase(execution, PlayPhase::Resolve)?;
        disclose_played_source(self, execution);
        let played = if execution.spell {
            let object = execution.ability.map_or_else(
                || execution.reference(self, &execution.context.source),
                |source| Ok(source.holder),
            )?;
            Fact::CardPlayed {
                execution: execution.id,
                object,
                controller,
            }
        } else {
            Fact::AbilityPlayed {
                execution: execution.id,
                source: execution
                    .ability
                    .ok_or_else(|| invalid("missing formal source"))?,
                qualification: play.qualification,
            }
        };
        execution.record_fact(self, played);
        if play.quick_activation {
            self.internal_record_quick_activation(execution.id, controller);
        }
        self.player_play_prefix(execution.began);
        if !execution.spell && play.qualification == AbilityQualification::UnionBurst {
            self.internal_commit_union_burst(execution.id, controller)?;
        }
        if play.qualification == AbilityQualification::UnionBurst {
            self.internal_ub_activated_events(execution)?;
        }
        if execution.spell {
            let receipt = play.card_play_payment_receipt(execution)?;
            self.internal_played_card_events(execution, receipt)?;
        }
        let paid = play
            .optional
            .iter()
            .filter(|payment| payment.accepted == Some(true))
            .map(|payment| payment.receipt)
            .collect();
        let empty_fusion = play.qualification == AbilityQualification::Fusion
            && execution
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Fused(event) if event.materials.is_empty()));
        if !empty_fusion {
            execution.prepend(selected_body(play.body, &play.chosen, &paid)?);
        }
        Ok(())
    }

    fn commit_ability_use(
        &mut self,
        execution: &Execution,
        play: &PlayState,
        source: RuntimeAbilitySource,
    ) -> Result<()> {
        let turn = TurnId(self.state.schedule.current()?);
        if let Some(limit) = play.limit {
            self.card_runtime
                .consume_quota(source.instance, turn, limit, play.cost_policy)?;
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
        Ok(())
    }

    fn internal_mode_material_question(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
        count: &Expr<Nat>,
    ) -> Result<bool> {
        let Some(binding) = public_numbers::material_count(count) else {
            return Ok(false);
        };
        let Some(index) = play.parameters.iter().position(|decision| {
            matches!(&decision.domain, ChoiceDomain::Objects { binding: candidate, .. }
                if *candidate == binding)
        }) else {
            return Ok(false);
        };
        let decision = play
            .parameters
            .remove(index)
            .ok_or_else(|| invalid("missing mode material parameter"))?;
        self.internal_play_material_parameter(execution, &decision, play)?;
        self.stamp_play_selection(execution)?;
        Ok(true)
    }

    fn internal_mode_count(
        &self,
        execution: &mut Execution,
        play: &PlayState,
        count: &Expr<Nat>,
    ) -> Result<Nat> {
        // The count may read this play's declaration, held by the detached play state.
        let detached = execution.play.replace(play.clone());
        let value = public_numbers::material_count(count).map_or_else(
            || execution.natural(self, count),
            |binding| execution.declared_material_count(binding),
        );
        execution.play = detached;
        value
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
            if self.internal_mode_material_question(execution, play, &group.count)? {
                play.modes.push_front(group);
                return Ok(true);
            }
            let count = self.internal_mode_count(execution, play, &group.count)?;
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
            let minimum = mode_minimum(*cardinality);
            let automatic = group.condition.is_some()
                && optional_payment::required_single_mode(&group.count, &group.chosen);
            let any = !automatic
                && minimum > 0
                && self.internal_choice_any(execution.context.controller)?;
            let source = execution
                .ability
                .ok_or_else(|| invalid("missing mode source"))?;
            let turn = TurnId(self.state.schedule.current()?);
            let mut legal = Vec::new();
            for mode in &group.modes {
                let parameters = mode.parameters.iter();
                if self.internal_mode_affordable(execution, play, mode)?
                    && self.internal_play_targets_feasible(execution, parameters)?
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
            let available = u64::try_from(legal.len()).map_err(invalid)?;
            // R-0015: "好きな数" widens the maximum only; a choice still takes at least one mode.
            let (count, minimum) = if any {
                (Nat(available), 1)
            } else {
                (count, minimum)
            };
            let affordable = self.internal_play_affordable(execution, play)?;
            if available < minimum || !affordable {
                return Err(crate::EngineFailure::IllegalPlay(
                    "no legal mode and cost plan".into(),
                ));
            }
            if count == Nat(0) {
                play.chosen.insert(group.chosen.id, Vec::new());
                continue;
            }
            if automatic {
                // The guard determines whether the sole branch declares its targets.
                play.chosen.insert(group.chosen.id, legal);
                continue;
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
        mode: &Mode,
    ) -> Result<bool> {
        let costs = &mode.costs;
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("mode costs"));
        };
        if steps.is_empty() {
            return Ok(true);
        }
        if play.exchange.is_some() {
            return Err(unsupported("exchange with mode costs"));
        }
        let spell = execution.spell;
        for mut trial in self.internal_public_material_probes(execution, play)? {
            for binding in mode.parameters.iter().filter_map(x_payment::parameter) {
                // Before the mode is selected, zero proves whether its fixed costs can be paid.
                trial.slots.insert(binding.key(), Slot::Natural(Nat(0)));
            }
            let mut probe = self.clone();
            probe.card_runtime.active_source = trial.ability;
            let paid = probe
                .internal_probe_hand_costs(&mut trial, &play.costs)
                .and_then(|()| probe.internal_card_payment(&mut trial, &play.costs, spell, None));
            let paid =
                paid.and_then(|()| probe.internal_card_payment(&mut trial, costs, false, None));
            match paid {
                Ok(()) => return Ok(true),
                Err(crate::EngineFailure::IllegalPlay(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    /// Public head-cost materials are chosen after the modes (BP09-071): one probe per legal
    /// way to fill them, so a payment counts as affordable when any of them succeeds.
    fn internal_public_material_probes(
        &self,
        execution: &Execution,
        play: &PlayState,
    ) -> Result<Vec<Execution>> {
        self.internal_public_cost_probes(execution, &play.parameters, |decision| {
            play.material_posture(decision)
        })
    }

    fn internal_public_cost_probes(
        &self,
        execution: &Execution,
        parameters: &VecDeque<Decision>,
        posture: impl Fn(&Decision) -> bool,
    ) -> Result<Vec<Execution>> {
        let mut probes = self.internal_quote_target_probes(execution, parameters)?;
        for decision in parameters {
            if !super::material::is_parameter(decision)
                || decision.visibility != (Visibility::Public {})
            {
                continue;
            }
            let choices =
                self.internal_public_material_choices(execution, decision, posture(decision))?;
            let mut next = Vec::new();
            for probe in &probes {
                for (key, slot) in &choices {
                    let mut filled = probe.clone();
                    filled.slots.insert(*key, slot.clone());
                    next.push(filled);
                }
            }
            probes = next;
        }
        Ok(probes)
    }

    fn internal_play_affordable(&self, execution: &Execution, play: &PlayState) -> Result<bool> {
        if play.exchange.is_some() {
            return self.internal_exchange_affordable(execution, play);
        }
        for mut probe in self.internal_public_material_probes(execution, play)? {
            let mut payment_game = self.clone();
            payment_game.card_runtime.active_source = execution.ability;
            let quoted = if play.quick_activation {
                self.internal_quick_activation_payment(&probe, &play.costs)?
                    .0
            } else {
                Cow::Borrowed(&play.costs)
            };
            match payment_game
                .internal_probe_hand_costs(&mut probe, &quoted)
                .and_then(|()| {
                    payment_game.internal_card_payment(&mut probe, &quoted, execution.spell, None)
                }) {
                Ok(()) => return Ok(true),
                Err(crate::EngineFailure::IllegalPlay(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
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
        actor: PlayerId,
        objects: Vec<ObjectRef>,
        roles: Vec<TargetRoleAnswer>,
    ) -> Result<()> {
        let play = execution
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing target play"))?;
        let snapshot = play
            .snapshot
            .clone()
            .ok_or_else(|| invalid("missing target snapshot"))?;
        let declaration = TargetDeclaration {
            player: actor,
            decision: wait.decision.id,
            objects: objects.clone(),
            roles,
        };
        execution.bind_targets(wait, objects);
        execution.captures(&snapshot, &wait.decision.captures)?;
        record_target_declaration(execution, declaration);
        Ok(())
    }

    fn retry_play_targets(execution: &mut Execution, play: &mut PlayState) -> Result<()> {
        if execution.probing_declaration() {
            return Err(crate::EngineFailure::IllegalPlay(
                "target choices require a retry".into(),
            ));
        }
        let restart = play
            .restart
            .as_ref()
            .ok_or_else(|| invalid("missing target rollback"))?;
        // Feasibility was proven against this same snapshot before the first target question.
        execution.slots.clone_from(&restart.slots);
        execution.targets.clear();
        // A distribution retry invalidates the entire previous round of target answers.
        discard_selection_round(execution);
        execution.selected_target_names.clear();
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

fn resolved_play(
    progress: super::super::Progress,
    source: Option<RuntimeAbilitySource>,
    spell: bool,
) -> Result<CardPlay> {
    if spell && source.is_none() && matches!(progress.facts.last(), Some(Fact::CardResolved { .. }))
    {
        return Ok(CardPlay::ResolvedCard(progress));
    }
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
    Ok(CardPlay::Resolved(Activation {
        execution,
        source,
        progress,
        resume: ResumeStage::CheckTiming,
    }))
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
    pub(super) fn prepare_intrinsic_card_play(&self, execution: &mut Execution) -> Result<()> {
        execution.field_entry = !execution.spell;
        let additional = self.internal_additional(execution)?;
        let parameters = additional
            .as_deref()
            .map_or_else(VecDeque::new, |(_, payment)| {
                payment.play.parameters.clone().into()
            });
        execution.play = Some(PlayState {
            cost_policy: AbilityCostPolicy::Normal,
            qualification: AbilityQualification::Ordinary,
            quick_activation: false,
            additional,
            resource_entry: None,
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: VecDeque::new(),
            groups: vec![],
            modes_prepared: false,
            chosen: BTreeMap::new(),
            parameters,
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
        enter_play_phase(execution, PlayPhase::Declare)?;
        super::super::processing::disclose_declared_source(self, execution);
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
