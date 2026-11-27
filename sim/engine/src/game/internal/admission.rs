use super::tokens::PaidX;
use crate::ir::repeated_moves as ir_repeated_moves;
use crate::ir::{AbilityId, InstructionId, ScopeId, WellFormedCardProgram};
mod plan;
pub(crate) use plan::{AdmittedCard, CandidateAdmission};
pub(super) use plan::{AdmittedProgram, BodyPlan, PlannedAbility};

pub(super) fn compile(program: &WellFormedProgram, source: &SourceSpan) -> Result<AdmittedProgram> {
    check(program, source)?;
    AdmittedProgram::compile(program, source)
}

pub(super) fn compile_card(
    game: &super::Game,
    number: &str,
    program: &WellFormedCardProgram,
) -> Result<AdmittedCard> {
    game.check_internal_card(number, program, None)?;
    AdmittedCard::compile(program)
}

pub(super) fn admit_candidate(
    game: &super::Game,
    number: &str,
    accepted: &CandidateAdmission,
) -> Result<Arc<AdmittedCard>> {
    game.check_internal_card(number, &accepted.candidate().program, Some(accepted))?;
    accepted.compiled()
}

use super::{
    Cardinality, ChoiceDomain, Control, DecisionKind, Declinable, InstructionContext, Located,
    Node, Program, Requirements, Result, SourceSpan, Visibility, WellFormedProgram, unsupported,
};
use crate::ir::delayed as delayed_ir;
use crate::ir::dice as ir_dice;
use crate::ir::events::subject::SubjectFields;
use crate::ir::events::{drive, equipment};
use crate::ir::flip_cost;
use crate::ir::hand_position;
use crate::ir::observation_selection::{private_hand_choice, private_look_choice, revealed_choice};
use crate::ir::player_rules::PlayerRule;
use crate::ir::player_stages;
use crate::ir::{
    AbilityOrigin, AbilitySource, Binding, CostSpec, Decision, EnterFieldEvent, KeywordId,
    Requirement,
};
use crate::ir::{Action, ActionPattern, DamageAmount, Effect, IntFrozen, IntOperation, Predicate};
use crate::ir::{Bool, DamageEvent, Expr, Int, ObjectRef};
use crate::ir::{EventPattern, TriggerCondition, TriggerSpec};
use crate::ir::{NumericChange, play_restrictions};
use crate::ir::{PlayerRef, PlayerStage, RepeatBody, Selector};
use crate::ir::{
    advance_entry, effect_evolution, phase_restrictions, prevention, printed_keywords,
};
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use core::num::NonZeroU32;

pub(super) fn check(program: &WellFormedProgram, source: &SourceSpan) -> Result<()> {
    check_body(program.program(), source, None, None, None, None, None)
}

pub(super) fn check_card(
    program: &Program,
    source: &SourceSpan,
    origin: &AbilitySource,
    trigger: Option<&TriggerSpec>,
) -> Result<()> {
    check_card_with_x(program, source, origin, trigger, None)
}

pub(super) fn check_card_with_x(
    program: &Program,
    source: &SourceSpan,
    origin: &AbilitySource,
    trigger: Option<&TriggerSpec>,
    x: Option<PaidX>,
) -> Result<()> {
    let (entry, damage) = match trigger.map(|trigger| &trigger.on) {
        Some(TriggerCondition::Event {
            pattern: EventPattern::EnterField { binding, .. },
        }) => (Some(*binding), None),
        Some(TriggerCondition::Event {
            pattern: EventPattern::Damage { binding, .. },
        }) => (None, Some(*binding)),
        _ => (None, None),
    };
    let subject = trigger.and_then(|trigger| {
        let TriggerCondition::Event { pattern } = &trigger.on else {
            return None;
        };
        SubjectFields::from_pattern(pattern)
            .map(SubjectFields::object)
            .or_else(|| equipment::binding(pattern).map(equipment::holder))
            .or_else(|| drive::checked_binding(pattern).map(drive::holder))
    });
    check_body(
        program,
        source,
        Some(origin),
        entry,
        damage,
        subject.as_ref(),
        x,
    )
}

pub(super) fn check_play(program: &Program, source: &SourceSpan, source_bound: bool) -> Result<()> {
    let located = Located {
        node: program.clone(),
        source: source.clone(),
    };
    let requirements = Requirements::derive_program(&located);
    // Native card costs precede source binding; ability declarations already bind their source.
    if !source_bound
        && requirements
            .required
            .iter()
            .any(|requirement| requirement.capability.code == "reference.source-information")
    {
        return Err(unsupported("play-time source information"));
    }
    check_requirements(program, true, &BTreeSet::new(), None, requirements)
}

fn check_body(
    program: &Program,
    source: &SourceSpan,
    origin: Option<&AbilitySource>,
    entry: Option<Binding<EnterFieldEvent>>,
    damage: Option<Binding<DamageEvent>>,
    subject: Option<&Expr<ObjectRef>>,
    x: Option<PaidX>,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    if let Some(origin) = origin {
        source_paths(program, "", origin, &mut paths);
    }
    if let Some(event) = subject {
        super::card::triggers::event_operand::paths(program, "", event, &mut paths);
    }
    if let Some(event) = damage {
        damage_paths(program, "", event, &mut paths);
    }
    let mut inspected = program.clone();
    super::movement::mixed::mask(&mut inspected)?;
    if let Some(origin) = origin {
        source_paths(&inspected, "", origin, &mut paths);
    }
    if let Some(event) = subject {
        super::card::triggers::event_operand::paths(&inspected, "", event, &mut paths);
    }
    super::history_turn::mask_public_history(&mut inspected);
    mask_delayed(&mut inspected)?;
    if let Some(origin) = origin {
        effect_evolution::mask(&mut inspected, origin);
    }
    if let Some(event) = entry {
        super::card::triggers::event_operand::mask_origin(&mut inspected, event);
    }
    let requirements = Requirements::derive_program(&Located {
        node: inspected.clone(),
        source: source.clone(),
    });
    check_requirements(&inspected, origin.is_some(), &paths, x, requirements)?;
    super::numeric::check_program(program)?;
    check_program(
        program,
        source,
        origin,
        &mut super::receipt_counts::ReceiptReads::default(),
        x,
    )
}

fn keyword_source_paths(
    action: &Action,
    path: &str,
    origin: &AbilitySource,
    paths: &mut BTreeSet<String>,
) {
    if !matches!(
        origin.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Fanfare
                | KeywordId::Lastword
                | KeywordId::OnAttack
                | KeywordId::OnEvolve
                | KeywordId::OnSuperEvolve
                | KeywordId::OnRace
                | KeywordId::OnDriveGain
                | KeywordId::Quick
                | KeywordId::AdvanceActivate
                | KeywordId::Stack,
            ..
        }
    ) {
        return;
    }
    paths.insert(format!("{path}/instruction/source/origin"));
    paths.insert(format!("{path}/instruction/source/origin/keyword"));
    let nested = if let Action::InstallEffect { effect, .. } = action
        && (prevention::projection(effect).is_some_and(|shape| shape.source == origin)
            || super::player_rules::check(action, origin).is_ok())
    {
        "effect/source"
    } else if let Action::GameResult { source, .. } = action
        && source == origin
    {
        "source"
    } else {
        return;
    };
    let prefix = format!("{path}/instruction/action/{nested}/origin");
    paths.insert(prefix.clone());
    paths.insert(format!("{prefix}/keyword"));
}

#[expect(
    clippy::too_many_lines,
    reason = "Every control body that can carry a source path shares one traversal."
)]
fn source_paths(
    program: &Program,
    path: &str,
    origin: &AbilitySource,
    paths: &mut BTreeSet<String>,
) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{path}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } if instruction.source == *origin => {
                if let Action::Grant { abilities, .. } = &instruction.action {
                    for (keyword_index, _) in abilities.iter().enumerate() {
                        paths.insert(format!(
                            "{path}/instruction/action/abilities/{keyword_index}/keyword"
                        ));
                    }
                }
                if super::movement::advance_entry(&instruction.action) {
                    paths.insert(format!("{path}/instruction/action/objects/zone"));
                }
                if play_restrictions::is_banish_play_installation(&instruction.action) {
                    for field in ["subject", "action", "polarity"] {
                        paths.insert(format!("{path}/instruction/action/effect/{field}"));
                    }
                }
                keyword_source_paths(&instruction.action, &path, origin, paths);
            }
            Node::Control {
                control:
                    Control::Pay {
                        costs: CostSpec::Ordered { steps },
                        on_paid,
                        ..
                    },
            } => {
                for (cost_index, cost_step) in steps.iter().enumerate() {
                    if cost_step.instruction.source == *origin {
                        let prefix = format!(
                            "{path}/control/costs/steps/{cost_index}/instruction/source/origin"
                        );
                        paths.insert(prefix.clone());
                        paths.insert(format!("{prefix}/keyword"));
                    }
                }
                source_paths(on_paid, &format!("{path}/control/on_paid"), origin, paths);
            }
            Node::Control {
                control:
                    Control::Repeat {
                        body: RepeatBody::Program { body },
                        ..
                    },
            } => source_paths(body, &format!("{path}/control/body/body"), origin, paths),
            Node::Control {
                control: Control::EachPlayer { stages, .. },
            } => {
                for (stage_index, stage) in stages.iter().enumerate() {
                    source_paths(
                        &stage.body,
                        &format!("{path}/control/stages/{stage_index}/body"),
                        origin,
                        paths,
                    );
                }
            }
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::May { body, .. }
                    | Control::EachObject { body, .. },
            } => source_paths(body, &format!("{path}/control/body"), origin, paths),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                source_paths(
                    then_body,
                    &format!("{path}/control/then_body"),
                    origin,
                    paths,
                );
                if let Some(body) = else_body {
                    source_paths(body, &format!("{path}/control/else_body"), origin, paths);
                }
            }
            Node::Control {
                control:
                    Control::Match {
                        leader_body,
                        follower_body,
                        ..
                    },
            } => {
                source_paths(
                    leader_body,
                    &format!("{path}/control/leader_body"),
                    origin,
                    paths,
                );
                source_paths(
                    follower_body,
                    &format!("{path}/control/follower_body"),
                    origin,
                    paths,
                );
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    if let CostSpec::Ordered { steps } = &mode.costs {
                        for (cost_index, cost_step) in steps.iter().enumerate() {
                            if cost_step.instruction.source == *origin {
                                keyword_source_paths(
                                    &cost_step.instruction.action,
                                    &format!(
                                        "{path}/control/modes/{mode_index}/costs/steps/{cost_index}"
                                    ),
                                    origin,
                                    paths,
                                );
                            }
                        }
                    }
                    source_paths(
                        &mode.body,
                        &format!("{path}/control/modes/{mode_index}/body"),
                        origin,
                        paths,
                    );
                }
            }
            _ => {}
        }
    }
}

fn parent_requirements(mut requirements: Requirements, card: bool) -> Requirements {
    let definitions = granted_definition_roots(&requirements, card);
    if definitions.is_empty() {
        return requirements;
    }
    for requirement in &mut requirements.required {
        if requirement.capability.version != NonZeroU32::MIN {
            continue;
        }
        // Nested definitions are checked independently, including shared capability names.
        requirement.locations.retain(|location| {
            !definitions.iter().any(|(root, prefix)| {
                location.ir_path == *root || location.ir_path.starts_with(prefix)
            })
        });
    }
    requirements
        .required
        .retain(|requirement| !requirement.locations.is_empty());
    requirements
}

fn granted_definition_roots(requirements: &Requirements, card: bool) -> Vec<(String, String)> {
    // The loader checks each nested definition as a separate ability entry.
    requirements
        .required
        .iter()
        .filter(|requirement| {
            card && requirement.capability.code == "shape.GrantedAbility.Definition"
        })
        .flat_map(|requirement| {
            requirement
                .locations
                .iter()
                .map(|location| (location.ir_path.clone(), format!("{}/", location.ir_path)))
        })
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "Every path-scoped capability exemption is listed in one auditable filter."
)]
fn check_requirements(
    program: &Program,
    card: bool,
    origins: &BTreeSet<String>,
    x: Option<PaidX>,
    requirements: Requirements,
) -> Result<()> {
    let mut requirements = parent_requirements(requirements, card);
    super::movement::mask_entry_effects(&mut requirements);
    let token_paths = super::tokens::paths(program, x);
    let kind_paths = super::by_kind::paths(program);
    let mut attack_targets = BTreeSet::new();
    super::card::triggers::event_operand::target_paths(program, "", &mut attack_targets);
    let missing = requirements
        .required
        .iter()
        .filter(|required| {
            required.capability.version != NonZeroU32::MIN
                || (!supported(required.capability.code)
                    && !required.locations.iter().all(|usage| {
                        let scoped = Requirement {
                            capability: required.capability,
                            locations: BTreeSet::from([usage.clone()]),
                        };
                        let requirement = &scoped;
                        super::tokens::requirement(requirement, &token_paths)
                            || super::by_kind::requirement(requirement, &kind_paths)
                            || requirement.locations.iter().all(|location| {
                                attack_targets.iter().any(|path| {
                                    location.ir_path == *path
                                        || location.ir_path.starts_with(&format!("{path}/"))
                                        || (matches!(
                                            requirement.capability.code,
                                            "value.aggregate.sum"
                                                | "shape.IntOperation.Sum"
                                                | "internal.NumericProperty.Life"
                                                | "internal.NumericProperty.Attack"
                                        ) && path.strip_suffix("/objects").is_some_and(
                                            |parent| {
                                                location.ir_path == parent
                                                    || location.ir_path
                                                        == format!("{parent}/property")
                                            },
                                        ))
                                })
                            })
                            || super::source::count_zone(&requirements, requirement)
                            || super::history_turn::public_requirement(&requirements, requirement)
                            || super::card::prevention::requirement(&requirements, requirement)
                            || event_field_requirement(&requirements, requirement)
                            || (card && play_requirement(requirement.capability.code))
                            || (matches!(
                                requirement.capability.code,
                                "shape.ActionPattern.PlayCard"
                                    | "internal.Polarity.Allow"
                                    | "internal.ZoneKind.Evolve"
                                    | "shape.AbilityOrigin.Keyword"
                                    | "shape.ObjectFrozen.EnterFieldObject"
                                    | "shape.ObjectFrozen.EnterExObject"
                                    | "shape.ObjectFrozen.RaceSubject"
                                    | "shape.ObjectFrozen.UbActivatedHolder"
                                    | "shape.ObjectFrozen.EquipHolder"
                                    | "shape.ObjectFrozen.DriveCheckedHolder"
                                    | "shape.ObjectFrozen.AttackAttacker"
                                    | "shape.ObjectFrozen.EvolveSubject"
                                    | "shape.ObjectFrozen.TargetedObject"
                                    | "shape.ObjectFrozen.DestroyObjectBefore"
                                    | "shape.ObjectFrozen.LeaveFieldObjectBefore"
                                    | "shape.IntFrozen.DamageAmount"
                                    | "shape.ObjectView.EventSnapshot"
                                    | "value.event-snapshot"
                                    | "shape.ObjectView.LastKnown"
                                    | "reference.lki"
                                    | "internal.LkiReason.ZoneExit"
                                    | "shape.EventContext.LeaveField"
                                    | "internal.EventSnapshotPolicy.BeforeExit"
                                    | "internal.CharacteristicView.Original"
                                    | "reference.expression"
                                    | "shape.ObjectOperand.FromExpression"
                                    | "read.event-field"
                                    | "internal.KeywordId.Fanfare"
                                    | "internal.KeywordId.Lastword"
                                    | "internal.KeywordId.OnAttack"
                                    | "internal.KeywordId.OnEvolve"
                                    | "internal.KeywordId.OnSuperEvolve"
                                    | "internal.KeywordId.OnRace"
                                    | "internal.KeywordId.OnDriveGain"
                                    | "internal.KeywordId.Quick"
                                    | "internal.KeywordId.AdvanceActivate"
                                    | "internal.KeywordId.Ward"
                                    | "internal.KeywordId.Aura"
                                    | "internal.KeywordId.Storm"
                                    | "internal.KeywordId.Rush"
                                    | "internal.KeywordId.Bane"
                                    | "internal.KeywordId.Drain"
                                    | "internal.KeywordId.Intimidate"
                                    | "internal.KeywordId.Assail"
                                    | "internal.KeywordId.Stack"
                            ) && requirement
                                .locations
                                .iter()
                                .all(|location| origins.contains(&location.ir_path)))
                            || super::search::keyword_requirement(
                                &requirements,
                                requirement,
                                origins,
                            )
                            || super::search::private_requirement(requirement.capability.code)
                    }))
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(unsupported(&format!("requirements {missing:?}")));
    }
    Ok(())
}

fn play_requirement(code: &str) -> bool {
    matches!(
        code,
        "shape.Control.EachObject"
            | "shape.Predicate.PostureEquals"
            | "shape.PostureRead.Posture"
            | "read.state.posture"
            | "predicate.posture-equals"
            | "shape.ActionPattern.PlayAbility"
            | "internal.AbilityKind.Activated"
            | "control.each-object"
            | "shape.ObjectOperand.Selected"
            | "reference.selected-object"
            | "shape.NatFrozen.SnapshotCounter"
            | "read.snapshot-counter"
            | "shape.SnapshotFields.SourceInformation"
            | "shape.NatRead.Counter"
            | "counter.move"
            | "read.counter"
            | "internal.KeywordId.EarthRite"
            | "shape.Holder.Object"
            | "shape.Action.Counter"
            | "action.counter"
            | "shape.CounterOperation.Add"
            | "shape.CounterOperation.Remove"
            | "shape.CounterOperation.Move"
            | "shape.PaymentReference.CardPlay"
            | "internal.OwnerZone.Ex"
            | "shape.ObjectOperand.FollowEventMove"
            | "shape.MovementEvent.LeaveField"
            | "shape.MovementEvent.Discard"
            | "shape.MovementEvent.Banish"
            | "reference.trigger-follow"
            | "read.event-header.move-receipt"
            | "read.card-play.payment-complete"
            | "shape.BoolFrozen.EvolutionSucceeded"
            | "shape.BoolFrozen.PaymentComplete"
            | "read.receipt.evolution-succeeded"
            | "read.receipt.payment-complete"
            | "shape.ObjectView.SourceInformation"
            | "reference.source-information"
            | "shape.NatFrozen.SpellChain"
            | "read.spell-chain"
            | "shape.NatFrozen.NecroCharge"
            | "read.necro-charge"
            | "internal.SpellCountTime.AtPlay"
            | "internal.SpellCountTime.AtResolution"
            | "shape.CaptureTime.AtPlay"
            | "shape.CaptureTime.AtSelection"
            | "shape.ExecutionRef.Current"
            | "shape.Predicate.NameContains"
            | "predicate.name-contains"
            | "shape.ChoiceDomain.Objects"
            | "shape.ChoiceDomain.Recipients"
            | "shape.RecipientSet.Bound"
            | "internal.Disjointness.Disjoint"
            | "shape.DecisionKind.PlayParameter"
            | "decision.play-parameter"
            | "decision.target-roles"
            | "decision.role-if-nonempty"
            | "decision.role-remainder"
            | "shape.RoleCardinality.Fixed"
            | "shape.RoleCardinality.IfNonempty"
            | "shape.RoleCardinality.Remainder"
            | "internal.PlaySlot.Targets"
            | "internal.PlaySlot.CostMaterials"
            | "internal.PlaySlot.Distribution"
            | "internal.PlaySlot.X"
            | "internal.SelectionSnapshot.AtDecision"
            | "internal.InputMode.Inline"
            | "shape.Control.Mode"
            | "control.mode"
            | "shape.ChoiceDomain.Modes"
            | "shape.ModeDecisionKind.PlayParameter"
            | "internal.ModeChoiceKind.ChoiceKeyword"
            | "internal.ModeChoiceKind.ResolutionAlternative"
            | "shape.ModeDecisionKind.ResolutionChoice"
            | "action.play-card"
            | "shape.Action.PlayCard"
            | "action.play-ability"
            | "shape.Action.PlayAbility"
            | "shape.AbilityOperand.Selected"
            | "shape.ChoiceDomain.Abilities"
            | "internal.AbilityChoiceKind.EvolutionTime"
            | "internal.AbilityChoiceKind.UnionBurst"
            | "internal.AbilityCostPolicy.Normal"
            | "internal.AbilityCostPolicy.WaiveOriginal"
            | "internal.AbilityTriggerPolicy.IgnoreAsAuthorized"
            | "shape.CostOverride.SetBase"
            | "internal.ModeExclusion.CurrentTurnAbilityInstance"
    )
}

fn check_action(
    action: &Action,
    source: &AbilitySource,
    span: &SourceSpan,
    x: Option<PaidX>,
) -> Result<()> {
    super::dice::check_action(action)?;
    super::declaration::check(action)?;
    super::victory::check_action(action, source)?;
    super::scheduled_turn::check_action(action)?;
    if matches!(action, Action::InstallEffect { effect, .. } if PlayerRule::from_effect(effect).is_some())
    {
        return super::player_rules::check(action, source).map(|_| ());
    }
    if phase_restrictions::projection(action).is_some() {
        return Ok(());
    }
    if let Action::InstallEffect { effect, .. } = action
        && matches!(effect.as_ref(), Effect::Replacement { .. })
    {
        return super::card::prevention::check_install(action, source);
    }
    if matches!(action, Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Permission { action: ActionPattern::Attack { .. }, .. }))
    {
        return super::card::attack_restrictions::check_install(action);
    }
    if matches!(action, Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Permission { action: ActionPattern::PlayCard { .. }, .. }))
    {
        return super::card::play_restrictions::check_install(action);
    }
    super::card::nested::check(action)?;
    super::card::nested::ability::check_action(action)?;
    if matches!(action, Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Permission { action: ActionPattern::PlayAbility { .. }, .. }))
    {
        return super::card::ability_restrictions::check_install(action);
    }
    super::search::check_action(action)?;
    super::tokens::check_action(action, span, x)?;
    super::tokens::equipment::check_action(action)?;
    super::transform::check_action(action)?;
    super::movement::check_action(action)?;
    super::flip::check_action(action)?;
    super::transfer::check_action(action)?;
    super::resource::check_action(action)?;
    super::counters::check_action(action)?;
    super::discard::check_action(action)?;
    super::numeric::check_action(action)?;
    super::numeric::pilot::check_action(action)?;
    super::grant::check_action(action)
}

fn check_player_stage(
    control: &Control,
    stages: &[PlayerStage],
    source: &SourceSpan,
    origin: Option<&AbilitySource>,
    reads: &mut super::receipt_counts::ReceiptReads,
) -> Result<()> {
    super::tokens::check_each(control)?;
    for stage in stages {
        let mut body = stage.body.clone();
        if player_stages::procedure(control).is_some() {
            for step in &mut body.steps {
                if let Node::Instruction { instruction } = &mut step.node {
                    instruction.actor = PlayerRef::Controller {};
                    match &mut instruction.action {
                        Action::Draw { player, .. } => *player = PlayerRef::Controller {},
                        Action::Shuffle { deck, .. } => *deck = PlayerRef::Controller {},
                        Action::Move { objects, .. } => {
                            if let Selector::Ordered { zone, .. } = objects {
                                zone.player = PlayerRef::Controller {};
                            } else {
                                *objects = Selector::Objects { objects: vec![] };
                            }
                        }
                        _ => return Err(unsupported("checked player stage")),
                    }
                }
            }
        }
        if super::movement::player_banish(control).is_some() {
            // Validate the private operand as an own-deck read only inside this checked stage.
            for step in &mut body.steps {
                if let Node::Instruction { instruction } = &mut step.node
                    && let Action::Banish {
                        objects: Selector::Ordered { zone, .. },
                    } = &mut instruction.action
                {
                    zone.player = PlayerRef::Controller {};
                    instruction.actor = PlayerRef::Controller {};
                }
            }
        }
        check_program(&body, source, origin, reads, None)?;
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Every admitted control shape is dispatched in one exhaustive match."
)]
fn check_program(
    program: &Program,
    source: &SourceSpan,
    origin: Option<&AbilitySource>,
    reads: &mut super::receipt_counts::ReceiptReads,
    x: Option<PaidX>,
) -> Result<()> {
    if !hand_position::valid(program) {
        return Err(unsupported("paired private hand and deck-edge choice"));
    }
    let card = origin.is_some();
    for step in &program.steps {
        if !matches!(&step.node, Node::Instruction { instruction } if matches!(instruction.action, Action::Delay { .. }))
        {
            super::ordering::check_node_with_x(&reads.mask(&step.node)?, source, x)?;
        }
        match &step.node {
            Node::Control { control }
                if card && effect_evolution::projection(control).is_some() => {}
            Node::Control { control }
                if advance_entry::projection(control).is_some_and(|projection| {
                    origin.is_some_and(|expected| projection.instruction.source == *expected)
                }) => {}
            Node::Control {
                control: control @ Control::Pay { on_paid, .. },
            } if card => {
                super::card::optional::check_control(control)?;
                check_program(on_paid, source, origin, reads, x)?;
            }
            Node::Control {
                control: control @ Control::EachObject { body, .. },
            } if card && super::counters::iteration(control).is_some() => {
                super::counters::check_iteration(control)?;
                check_program(body, source, origin, reads, x)?;
            }
            Node::Control {
                control: control @ Control::EachPlayer { stages, .. },
            } => check_player_stage(control, stages, source, origin, reads)?,
            Node::Control {
                control: control @ Control::Repeat { .. },
            } => {
                let body = if let Some((body, _)) = ir_dice::repeat(control) {
                    body
                } else {
                    ir_repeated_moves::projection(control)
                        .ok_or_else(|| unsupported("repeat"))?
                        .body
                };
                check_program(body, source, origin, reads, x)?;
            }
            Node::Control {
                control: control @ Control::Simultaneous { .. },
            } => {
                let body = super::movement::mixed::inspection(control, program.scope)?;
                check_program(&body, source, origin, reads, x)?;
            }
            Node::Control {
                control: Control::Sequence { body },
            } => check_program(body, source, origin, reads, x)?,
            Node::Control {
                control: control @ Control::May { body, .. },
            } => {
                super::alternatives::check_may(control)?;
                check_program(body, source, origin, reads, x)?;
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_program(then_body, source, origin, reads, x)?;
                if let Some(body) = else_body {
                    check_program(body, source, origin, reads, x)?;
                }
            }
            Node::Control {
                control:
                    Control::Match {
                        leader_body,
                        follower_body,
                        ..
                    },
            } => {
                check_program(leader_body, source, origin, reads, x)?;
                check_program(follower_body, source, origin, reads, x)?;
            }
            Node::Instruction { instruction } => {
                if let Action::Delay { body, trigger, .. } = &instruction.action {
                    if !card || !delayed_ir::valid(&instruction.action) {
                        return Err(unsupported("delayed body"));
                    }
                    check_card(body, source, &instruction.source, Some(trigger))?;
                }
                super::dice::check_instruction(instruction)?;
                check_action(&instruction.action, &instruction.source, &step.source, x)?;
                if instruction.precondition.is_some()
                    || instruction.context != InstructionContext::Effect
                {
                    return Err(unsupported("instruction prerequisite or cost"));
                }
            }
            Node::Decision { decision } => check_decision(decision, source, card)?,
            Node::Control {
                control: Control::Mode { modes, .. },
            } if card => {
                for mode in modes {
                    check_program(&mode.body, source, origin, reads, None)?;
                }
            }
            Node::Capture { .. } => {}
            Node::Control { .. } => return Err(unsupported("control")),
        }
        reads.record(&step.node);
    }
    Ok(())
}

fn check_decision(decision: &Decision, source: &SourceSpan, card: bool) -> Result<()> {
    if matches!(decision.domain, ChoiceDomain::Abilities { .. }) {
        if !card {
            return Err(unsupported("ability choice outside a whole card"));
        }
        super::card::nested::ability::check_choice(decision)?;
    }
    if matches!(
        decision.domain,
        ChoiceDomain::Objects { .. } | ChoiceDomain::Recipients { .. }
    ) {
        if !card {
            return Err(unsupported("resolution field choice outside a whole card"));
        }
        super::card::selection::check_resolution(decision)?;
    }
    if matches!(decision.domain, ChoiceDomain::Distribution { .. }) && source.card_no != "synthetic"
    {
        return Err(unsupported(
            "resolution distribution without reviewed timing/minimum evidence",
        ));
    }
    let public_required = matches!(decision.visibility, Visibility::Public {})
        && matches!(
            decision.kind,
            DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                precondition: None,
                ..
            }
        );
    let stack_rule = printed_keywords::is_stack_increase_choice(decision);
    // A public reveal choice may also be declined, like a choice among looked cards.
    let private_look =
        private_look_choice(decision) || private_hand_choice(decision) || revealed_choice(decision);
    let private_flip = flip_cost::private_resolution(decision);
    if !matches!(
        decision.domain,
        ChoiceDomain::Integer {
            cardinality: Cardinality {
                maximum: Some(_),
                ..
            },
            ..
        } | ChoiceDomain::Distribution { .. }
            | ChoiceDomain::Abilities { .. }
            | ChoiceDomain::Objects { .. }
            | ChoiceDomain::Recipients { .. }
    ) || !decision.captures.is_empty()
        || !(public_required || private_look || private_flip || stack_rule)
    {
        return Err(unsupported("decision metadata"));
    }
    Ok(())
}

fn event_field_requirement(requirements: &Requirements, requirement: &Requirement) -> bool {
    requirement.capability.code == "read.event-field"
        && requirement.locations.iter().all(|location| {
            super::history_turn::public_location(requirements, location)
                || requirements.required.iter().any(|other| {
                    matches!(
                        other.capability.code,
                        "shape.NatFrozen.DiceValue"
                            | "shape.IntFrozen.MoveBatchCount"
                            | "shape.BoolFrozen.EntryAdditionalMaterial"
                            | "shape.EntryMethodFields.EnterFieldMethod"
                            | "shape.TraitsFrozen.EnterFieldAbilitySourceTraits"
                            | "shape.EntryOriginFields.EnterFieldFrom"
                    ) && other
                        .locations
                        .iter()
                        .any(|at| at.ir_path == location.ir_path)
                })
        })
}

// Every enum arm, internal tag and nested requirement is opt-in, including dead branches.
// Insert new codes in lexicographic order to keep additions easy to review.
#[expect(
    clippy::too_many_lines,
    reason = "The exact supported derivation codes are reviewed as one closed set."
)]
fn supported(code: &str) -> bool {
    matches!(
        code,
        "action.banish"
            | "action.box-pilot"
            | "action.damage"
            | "action.declare"
            | "action.destroy"
            | "action.dice"
            | "action.discard"
            | "action.draw"
            | "action.equip"
            | "action.flip-down"
            | "action.flip-up"
            | "action.game-result"
            | "action.grant"
            | "action.install-effect"
            | "action.life"
            | "action.limit-break"
            | "action.look"
            | "action.meal-race"
            | "action.modify"
            | "action.move"
            | "action.order"
            | "action.posture"
            | "action.random"
            | "action.remove-ability"
            | "action.resource"
            | "action.reveal"
            | "action.reveal-until"
            | "action.ride-drive"
            | "action.schedule-turn"
            | "action.shuffle"
            | "action.token"
            | "action.transfer"
            | "action.transform"
            | "cardinality.bounded"
            | "cardinality.unbounded"
            | "control.if"
            | "control.may"
            | "control.pay"
            | "control.repeat"
            | "control.repeat.export"
            | "control.sequence"
            | "damage.allocation"
            | "decision.input"
            | "decision.resolution-choice"
            | "decision.rule-choice"
            | "discard.all"
            | "discard.chosen"
            | "discard.chosen-count"
            | "discard.random"
            | "duration.next-phase-edge"
            | "duration.next-phase-interval"
            | "duration.next-turn-end"
            | "duration.next-turn-edge"
            | "duration.next-turn-interval"
            | "duration.no-expiry"
            | "duration.resolution"
            | "duration.source-zone-exit"
            | "duration.turn"
            | "effect.continuous"
            | "effect.rule-modifier"
            | "effect.cost-delta"
            | "effect.cost-modifier"
            | "effect.cost-set"
            | "effect.permission"
            | "event.match.Play"
            | "event.match.QuickActivation"
            | "event.match.Targeted"
            | "event.payment.earth-rite"
            | "history.sum.earth-rite-stack"
            | "history.sum.life-delta"
            | "internal.Arithmetic.Add"
            | "internal.Arithmetic.FloorDivide"
            | "internal.Arithmetic.HalfUp"
            | "internal.Arithmetic.Maximum"
            | "internal.Arithmetic.Minimum"
            | "internal.Arithmetic.Multiply"
            | "internal.Arithmetic.Subtract"
            | "internal.BooleanState.Boxed"
            | "internal.BooleanState.EnteredThisTurn"
            | "internal.BooleanState.FaceUp"
            | "internal.BooleanState.HasTrigger"
            | "internal.BooleanState.Racing"
            | "internal.CapacityProperty.Capacity"
            | "internal.CapacityProperty.Free"
            | "internal.CapacityZone.Ex"
            | "internal.CapacityZone.Field"
            | "internal.CardKind.Crest"
            | "internal.CardKind.EvolveFollower"
            | "internal.CardKind.Treasure"
            | "internal.CardSnapshot.CurrentVerifiedSnapshot"
            | "internal.CharacteristicView.Effective"
            | "internal.Comparison.Equal"
            | "internal.Comparison.Greater"
            | "internal.Comparison.GreaterEqual"
            | "internal.Comparison.Less"
            | "internal.Comparison.LessEqual"
            | "internal.Comparison.NotEqual"
            | "internal.ContinuousLayer.Numeric"
            | "internal.CostKind.Evolution"
            | "internal.CostKind.Play"
            | "internal.CostKind.QuickActivation"
            | "internal.DamageFlags.Ability"
            | "internal.Declinable.MayDecline"
            | "internal.Declinable.Required"
            | "internal.Edge.Begin"
            | "internal.Edge.End"
            | "internal.EntryMethod.AbilityPut"
            | "internal.EntryMethod.Play"
            | "internal.EntryMethod.RulePut"
            | "internal.EvolutionState.Base"
            | "internal.EvolutionState.Evolved"
            | "internal.EvolutionState.SuperEvolved"
            | "internal.ExtremumKind.Maximum"
            | "internal.ExtremumKind.Minimum"
            | "internal.GenerationDeduplication.ByObjectAndGeneration"
            | "internal.HistoryCount.Facts"
            | "internal.HistoryCount.Subjects"
            | "internal.InstallationTime.InstructionStart"
            | "internal.InstructionContext.Cost"
            | "internal.InstructionContext.Effect"
            | "internal.IterationCheck.AfterIteration"
            | "internal.IterationCheck.BeforeIteration"
            | "internal.LimitBreakState.Final"
            | "internal.LimitBreakState.FinalPossible"
            | "internal.LimitBreakState.Normal"
            | "internal.MealRaceOperation.Race"
            | "internal.MemberProjection.After"
            | "internal.MemberProjection.Before"
            | "internal.MoveReason.Put"
            | "internal.NumericProperty.Attack"
            | "internal.NumericProperty.Cost"
            | "internal.NumericProperty.Count"
            | "internal.NumericProperty.Life"
            | "internal.NumericProperty.OriginalCost"
            | "internal.ObjectProperty.Class"
            | "internal.ObjectProperty.Cost"
            | "internal.ObjectProperty.Names"
            | "internal.Outcome.Win"
            | "internal.Outcome.Lose"
            | "internal.OwnerZone.Banish"
            | "internal.OwnerZone.Cemetery"
            | "internal.OwnerZone.Deck"
            | "internal.OwnerZone.Hand"
            | "internal.PaymentOptionality.Optional"
            | "internal.PaymentOptionality.Required"
            | "internal.PaymentTime.PlayParameter"
            | "internal.PaymentTime.ResolutionChoice"
            | "internal.Phase.Main"
            | "internal.Phase.Start"
            | "internal.PlayerRelation.All"
            | "internal.PlayerRelation.Opponents"
            | "internal.PlayerRelation.SelfPlayer"
            | "internal.PlayerResource.Ep"
            | "internal.PlayerResource.Life"
            | "internal.PlayerResource.PpCurrent"
            | "internal.PlayerResource.PpMaximum"
            | "internal.PlayerResource.Sep"
            | "internal.Polarity.Forbid"
            | "internal.Posture.Acted"
            | "internal.Posture.Standing"
            | "internal.QuantityUnit.Cards"
            | "internal.RandomnessPolicy.FreshRequest"
            | "internal.Resource.Ep"
            | "internal.Resource.PpCurrent"
            | "internal.Resource.PpMaximum"
            | "internal.ResourceOperation.Add"
            | "internal.ResourceOperation.Pay"
            | "internal.RevealUntilMembers.Matched"
            | "internal.RevealUntilMembers.Revealed"
            | "internal.Role.Controller"
            | "internal.Role.Owner"
            | "internal.RuleProcedure.StackRecipient"
            | "internal.SliceDirection.Forward"
            | "internal.Stat.Attack"
            | "internal.Stat.Life"
            | "internal.StepwiseMode.Exact"
            | "internal.StepwiseMode.UpToStepwise"
            | "internal.StepwiseTiming.BeforeDraw"
            | "internal.TextQuantifier.All"
            | "internal.TextQuantifier.Implicit"
            | "internal.TextQuantifier.One"
            | "internal.UsageReference.CurrentRulePermission"
            | "internal.ZoneKind.Banish"
            | "internal.ZoneKind.Cemetery"
            | "internal.ZoneKind.Deck"
            | "internal.ZoneKind.Evolve"
            | "internal.ZoneKind.Ex"
            | "internal.ZoneKind.Field"
            | "internal.ZoneKind.Hand"
            | "numeric.add"
            | "numeric.set"
            | "numeric.set-original"
            | "player.require-one"
            | "predicate.and"
            | "predicate.compare-int"
            | "predicate.compare-nat"
            | "predicate.equal"
            | "predicate.evolution-equals"
            | "predicate.exists"
            | "predicate.has-info"
            | "predicate.name-equals"
            | "predicate.not"
            | "predicate.or"
            | "predicate.player-equals"
            | "read.card-play-origin"
            | "read.combo"
            | "read.attack.count.source"
            | "read.cost"
            | "read.crimson"
            | "read.field-entry-method"
            | "read.field-entry-origin"
            | "read.history.count"
            | "read.history.sum"
            | "read.limit-break"
            | "read.other-hand-count"
            | "read.player-resource"
            | "read.receipt.actual"
            | "read.receipt.discarded-cost"
            | "read.receipt.evolution-succeeded"
            | "read.receipt.member-any"
            | "read.receipt.member-count"
            | "read.receipt.members"
            | "read.receipt.paid-material-acts"
            | "read.receipt.paid-material-number"
            | "read.receipt.payment-complete"
            | "read.receipt.requested"
            | "read.roles"
            | "read.state"
            | "read.state.evolution"
            | "read.stats"
            | "read.turn.ordinal"
            | "read.zone-capacity"
            | "read.zone-count"
            | "receipt.iteration"
            | "receipt.local"
            | "receipt.move-after"
            | "receipt.move-after-all"
            | "recipient.explicit"
            | "recipient.leaders"
            | "recipient.objects"
            | "recipient.union"
            | "reference.follow-generation"
            | "reference.object-generation"
            | "reference.selected"
            | "reference.snapshot"
            | "rule.double-fanfare"
            | "rule.cannot-lose-defense"
            | "rule.unlimited-hand"
            | "selector.difference"
            | "selector.extremum"
            | "selector.filter"
            | "selector.intersection"
            | "selector.objects"
            | "selector.ordered"
            | "selector.union"
            | "selector.zone"
            | "shape.AbilityOrigin.Printed"
            | "shape.Action.Banish"
            | "shape.Action.BoxPilot"
            | "shape.Action.Damage"
            | "shape.Action.Declare"
            | "shape.Action.Destroy"
            | "shape.Action.Dice"
            | "shape.Action.Discard"
            | "shape.Action.Draw"
            | "shape.Action.Equip"
            | "shape.Action.FlipDown"
            | "shape.Action.FlipUp"
            | "shape.Action.GameResult"
            | "shape.Action.Grant"
            | "shape.Action.InstallEffect"
            | "shape.Action.Life"
            | "shape.Action.LimitBreak"
            | "shape.Action.Look"
            | "shape.Action.MealRace"
            | "shape.Action.Modify"
            | "shape.Action.Move"
            | "shape.Action.Order"
            | "shape.Action.Posture"
            | "shape.Action.Random"
            | "shape.Action.RemoveAbility"
            | "shape.Action.Resource"
            | "shape.Action.Reveal"
            | "shape.Action.RevealUntil"
            | "shape.Action.RideDrive"
            | "shape.Action.ScheduleTurn"
            | "shape.Action.Shuffle"
            | "shape.Action.Token"
            | "shape.Action.Transfer"
            | "shape.Action.Transform"
            | "shape.ActionPattern.Attack"
            | "shape.ActionPattern.Destroy"
            | "shape.ActionPattern.Draw"
            | "shape.ActionPattern.IncreasePpMaximum"
            | "shape.ActionPattern.PlayCardKind"
            | "shape.ActionPattern.Stand"
            | "shape.BatchPolicy.Instruction"
            | "shape.BoolFrozen.EntryAdditionalMaterial"
            | "shape.BoolRead.CardPlayOrigin"
            | "shape.BoolRead.Crimson"
            | "shape.BoolRead.FieldEntryMethod"
            | "shape.BoolRead.FieldEntryOrigin"
            | "shape.BoolRead.LimitBreak"
            | "shape.BoolRead.State"
            | "shape.BoxPilotMode.Boxed"
            | "shape.BoxPilotMode.Pilot"
            | "shape.Capture.Boolean"
            | "shape.Capture.Integer"
            | "shape.Capture.Name"
            | "shape.Capture.Names"
            | "shape.Capture.Natural"
            | "shape.Capture.Objects"
            | "shape.Capture.Player"
            | "shape.CaptureTime.AtResolution"
            | "shape.CausePattern.Effect"
            | "shape.CharacteristicPatch.AddTrait"
            | "shape.CharacteristicPatch.Combined"
            | "shape.CharacteristicPatch.Numeric"
            | "shape.CharacteristicPatch.SetKind"
            | "shape.ChoiceDomain.Distribution"
            | "shape.ChoiceDomain.Integer"
            | "shape.Control.If"
            | "shape.Control.May"
            | "shape.Control.Pay"
            | "shape.Control.Repeat"
            | "shape.Control.Sequence"
            | "shape.CostChange.Delta"
            | "shape.CostChange.Set"
            | "shape.CostSpec.Ordered"
            | "shape.CostStep"
            | "shape.DamageAmount.Allocation"
            | "shape.DamageAmount.Uniform"
            | "shape.DamageRecipient.Leader"
            | "shape.DamageRecipient.Object"
            | "shape.DecisionKind.ResolutionChoice"
            | "shape.DecisionKind.RuleChoice"
            | "shape.Declaration.BoundedInteger"
            | "shape.Declaration.CardNames"
            | "shape.Declaration.NonnegativeInteger"
            | "shape.DiceOperation.Roll"
            | "shape.DiscardMode.All"
            | "shape.DiscardMode.Chosen"
            | "shape.DiscardMode.ChosenCount"
            | "shape.DiscardMode.Random"
            | "shape.DurationEnd.CurrentTurnEnd"
            | "shape.DurationEnd.EndResolution"
            | "shape.DurationEnd.NextPhaseEdge"
            | "shape.DurationEnd.NextTurnEnd"
            | "shape.DurationEnd.NextTurnEdge"
            | "shape.DurationEnd.NoTemporalExpiry"
            | "shape.DurationEnd.SourceZoneExit"
            | "shape.Effect.Continuous"
            | "shape.Effect.CostModifier"
            | "shape.Effect.Permission"
            | "shape.Effect.RuleModifier"
            | "shape.EffectReference.CurrentInstallation"
            | "shape.EffectSource.Any"
            | "shape.EffectTarget.Dynamic"
            | "shape.EntryMethodFields.EnterFieldMethod"
            | "shape.EntryOrigin.Zone"
            | "shape.EntryOriginFields.EnterFieldFrom"
            | "shape.EventPattern.Play"
            | "shape.EventPattern.QuickActivation"
            | "shape.EvolutionRead.Evolution"
            | "shape.ExecutionRef.Current"
            | "shape.Expr.Bound"
            | "shape.Expr.Captured"
            | "shape.Expr.Compute"
            | "shape.Expr.Frozen"
            | "shape.Expr.Literal"
            | "shape.Expr.Live"
            | "shape.FollowPermission.SameEffectExplicitReference"
            | "shape.GrantedAbility.Keyword"
            | "shape.HistoryCutoff.CurrentPoint"
            | "shape.HistoryRange.CurrentTurn"
            | "shape.InclusionPolicy.IncludeAll"
            | "shape.InstallationSubjects.Objects"
            | "shape.InstallationSubjects.Players"
            | "shape.IntFrozen.DiscardedCost"
            | "shape.IntFrozen.MoveBatchCount"
            | "shape.IntFrozen.PaidMaterialActs"
            | "shape.IntFrozen.PaidMaterialNumber"
            | "shape.IntFrozen.ReceiptRequested"
            | "shape.IntOperation.Arithmetic"
            | "shape.IntOperation.Conditional"
            | "shape.IntOperation.Extremum"
            | "shape.IntOperation.FromNat"
            | "shape.IntOperation.InformationOrZero"
            | "shape.IntOperation.Sum"
            | "shape.IntRead.HistorySum"
            | "shape.IntRead.PlayerResource"
            | "shape.Interval.DuringNextPhase"
            | "shape.Interval.DuringNextTurn"
            | "shape.MeasuredReceipt.Cards"
            | "shape.MeasuredReceipt.Counters"
            | "shape.MeasuredReceipt.Damage"
            | "shape.MeasuredReceipt.Payment"
            | "shape.MoveDestination.Owners"
            | "shape.MovementReceipt.Banish"
            | "shape.MovementReceipt.Destroy"
            | "shape.MovementReceipt.Discard"
            | "shape.MovementReceipt.Move"
            | "shape.MovementReceipt.Order"
            | "shape.MovementReceipt.Search"
            | "shape.MovementReceiptList.Move"
            | "shape.NameOperation.UniqueName"
            | "shape.NatFrozen.DiceValue"
            | "shape.NatFrozen.ReceiptActual"
            | "shape.NatOperation.CheckedInt"
            | "shape.NatOperation.Conditional"
            | "shape.NatOperation.Count"
            | "shape.NatOperation.DistinctCount"
            | "shape.NatOperation.ReceiptCount"
            | "shape.NatRead.Combo"
            | "shape.NatRead.SourceAttacksThisTurn"
            | "shape.NatRead.HistoryCount"
            | "shape.NatRead.OtherHandCount"
            | "shape.NatRead.TurnOrdinal"
            | "shape.NatRead.ZoneCapacity"
            | "shape.NatRead.ZoneCount"
            | "shape.Node.Capture"
            | "shape.Node.Control"
            | "shape.Node.Decision"
            | "shape.Node.Instruction"
            | "shape.NumberRead.Cost"
            | "shape.NumberRead.Stats"
            | "shape.NumericChange.Add"
            | "shape.NumericChange.Set"
            | "shape.NumericChange.SetOriginal"
            | "shape.NumericHistoryQuery.EarthRiteRemovedStack"
            | "shape.NumericHistoryQuery.LifeDelta"
            | "shape.ObjectOperand.SourceHolder"
            | "shape.ObjectReceipt.Flip"
            | "shape.ObjectReceipt.Fusion"
            | "shape.ObjectReceipt.Look"
            | "shape.ObjectReceipt.Movement"
            | "shape.ObjectReceipt.Random"
            | "shape.ObjectReceipt.Reveal"
            | "shape.ObjectReceipt.RevealUntil"
            | "shape.ObjectView.Current"
            | "shape.ObjectView.Selected"
            | "shape.ObjectView.Snapshot"
            | "shape.PaymentReference.Local"
            | "shape.PermissionSubject.Objects"
            | "shape.PermissionSubject.Players"
            | "shape.PlayerRead.Role"
            | "shape.PlayerRef.Active"
            | "shape.PlayerRef.Controller"
            | "shape.PlayerRef.FromExpression"
            | "shape.PlayerRef.Owner"
            | "shape.PlayerRef.RequireOne"
            | "shape.Position.Bottom"
            | "shape.Position.Index"
            | "shape.Position.Top"
            | "shape.Predicate.And"
            | "shape.Predicate.CompareInt"
            | "shape.Predicate.CompareNat"
            | "shape.Predicate.Equal"
            | "shape.Predicate.EvolutionEquals"
            | "shape.Predicate.Exists"
            | "shape.Predicate.HasInfo"
            | "shape.Predicate.NameEquals"
            | "shape.Predicate.Not"
            | "shape.Predicate.Or"
            | "shape.Predicate.PlayerEquals"
            | "shape.Predicate.ReceiptAny"
            | "shape.ReceiptExport.Move"
            | "shape.RecipientSet.Explicit"
            | "shape.RecipientSet.Leaders"
            | "shape.RecipientSet.Objects"
            | "shape.RecipientSet.Union"
            | "shape.RemoveScope.AllAtTime"
            | "shape.RepeatBody.Program"
            | "shape.RideDriveOperation.GainDrive"
            | "shape.RuleModifier.DoubleFanfare"
            | "shape.RuleModifier.CannotLoseDefense"
            | "shape.RuleModifier.UnlimitedHand"
            | "shape.ScheduleAnchor.NextTurn"
            | "shape.SelectionView.Current"
            | "shape.Selector.Bound"
            | "shape.Selector.Captured"
            | "shape.Selector.Difference"
            | "shape.Selector.Extremum"
            | "shape.Selector.Filter"
            | "shape.Selector.Intersection"
            | "shape.Selector.MoveAfter"
            | "shape.Selector.MoveAfterAll"
            | "shape.Selector.Objects"
            | "shape.Selector.Ordered"
            | "shape.Selector.ReceiptMembers"
            | "shape.Selector.Union"
            | "shape.Selector.Zone"
            | "shape.ShuffleScope.FullZone"
            | "shape.ShuffleScope.Subset"
            | "shape.StandScope.RuleStart"
            | "shape.Termination.Count"
            | "shape.Termination.Until"
            | "shape.Termination.While"
            | "shape.TimeAnchor.Installation"
            | "shape.TimestampBasis.ProperPlay"
            | "shape.TokenAllocation.EachExactlyOnce"
            | "shape.TokenAllocation.Single"
            | "shape.TokenDestination.Single"
            | "shape.TokenDestination.Split"
            | "shape.TokenInitialState.StackCounters"
            | "shape.TokenTemplate.Fixed"
            | "shape.TokenTemplate.Name"
            | "shape.TraitsFrozen.EnterFieldAbilitySourceTraits"
            | "shape.TurnOperation.InsertBefore"
            | "shape.TurnOperation.SkipNext"
            | "shape.UsageScope.SharedPlayer"
            | "shape.UseLimit"
            | "shape.ValueEquality.EntryMethod"
            | "shape.ValueEquality.EntryOrigin"
            | "shape.Visibility.Only"
            | "shape.Visibility.Public"
            | "shuffle.subset"
            | "token.dynamic-name"
            | "transform.token-allocation"
            | "usage.shared-player"
            | "value.absent-number-zero"
            | "value.aggregate.count"
            | "value.aggregate.distinct-count"
            | "value.aggregate.extremum"
            | "value.aggregate.sum"
            | "value.capture"
            | "value.checked-nat"
            | "value.conditional"
            | "value.live"
            | "value.nat-to-int"
            | "value.signed-arithmetic"
            | "value.signed-literal"
            | "value.unique-name"
    )
}

fn damage_paths(
    program: &Program,
    path: &str,
    event: Binding<DamageEvent>,
    paths: &mut BTreeSet<String>,
) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{path}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => match &instruction.action {
                Action::Draw { count, .. } => damage_integer(
                    count,
                    &format!("{path}/instruction/action/count"),
                    event,
                    paths,
                ),
                Action::Damage {
                    amount: DamageAmount::Uniform { amount },
                    ..
                } => damage_integer(
                    amount,
                    &format!("{path}/instruction/action/amount/amount"),
                    event,
                    paths,
                ),
                _ => {}
            },
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
            } => damage_paths(body, &format!("{path}/control/body"), event, paths),
            Node::Control {
                control:
                    Control::If {
                        condition,
                        then_body,
                        else_body,
                    },
            } => {
                damage_boolean(
                    condition,
                    &format!("{path}/control/condition"),
                    event,
                    paths,
                );
                damage_paths(
                    then_body,
                    &format!("{path}/control/then_body"),
                    event,
                    paths,
                );
                if let Some(body) = else_body {
                    damage_paths(body, &format!("{path}/control/else_body"), event, paths);
                }
            }
            _ => {}
        }
    }
}
fn damage_integer(
    value: &Expr<Int>,
    path: &str,
    event: Binding<DamageEvent>,
    paths: &mut BTreeSet<String>,
) {
    match value {
        Expr::Frozen(field) if matches!(field.as_ref(), IntFrozen::DamageAmount { event: bound } if *bound == event) =>
        {
            paths.insert(format!("{path}/frozen"));
        }
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::Arithmetic { arguments, .. } => {
                for (index, argument) in arguments.iter().enumerate() {
                    damage_integer(
                        argument,
                        &format!("{path}/compute/arguments/{index}"),
                        event,
                        paths,
                    );
                }
            }
            IntOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                damage_boolean(
                    condition,
                    &format!("{path}/compute/condition"),
                    event,
                    paths,
                );
                damage_integer(
                    then_value,
                    &format!("{path}/compute/then_value"),
                    event,
                    paths,
                );
                damage_integer(
                    else_value,
                    &format!("{path}/compute/else_value"),
                    event,
                    paths,
                );
            }
            _ => {}
        },
        _ => {}
    }
}
fn damage_boolean(
    value: &Expr<Bool>,
    path: &str,
    event: Binding<DamageEvent>,
    paths: &mut BTreeSet<String>,
) {
    let Expr::Compute(predicate) = value else {
        return;
    };
    match predicate.as_ref() {
        Predicate::CompareInt { left, right, .. } => {
            damage_integer(left, &format!("{path}/compute/left"), event, paths);
            damage_integer(right, &format!("{path}/compute/right"), event, paths);
        }
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            for (index, part) in predicates.iter().enumerate() {
                damage_boolean(
                    part,
                    &format!("{path}/compute/predicates/{index}"),
                    event,
                    paths,
                );
            }
        }
        Predicate::Not { predicate } => damage_boolean(
            predicate,
            &format!("{path}/compute/predicate"),
            event,
            paths,
        ),
        _ => {}
    }
}

fn mask_delayed(program: &mut Program) -> Result<()> {
    for step in &mut program.steps {
        match &mut step.node {
            Node::Instruction { instruction }
                if matches!(instruction.action, Action::Delay { .. }) =>
            {
                if !delayed_ir::valid(&instruction.action) {
                    return Err(unsupported("delayed subscription"));
                }
                instruction.action = Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(0)),
                    },
                };
            }
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                mask_delayed(then_body)?;
                if let Some(body) = else_body {
                    mask_delayed(body)?;
                }
            }
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::May { body, .. }
                    | Control::Pay { on_paid: body, .. },
            } => mask_delayed(body)?,
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    mask_delayed(&mut mode.body)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn drive_ability(
    keyword: KeywordId,
    id: AbilityId,
    scope: ScopeId,
    instruction: InstructionId,
    source: &SourceSpan,
) -> Result<PlannedAbility> {
    let node = printed_keywords::drive_definition(keyword, id, scope, instruction, source)
        .ok_or_else(|| super::invalid("Drive definition"))?;
    PlannedAbility::compile(&Located {
        node,
        source: source.clone(),
    })
}
pub(super) fn drain_ability(
    id: AbilityId,
    scope: ScopeId,
    instruction: InstructionId,
    source: &SourceSpan,
) -> Result<PlannedAbility> {
    let node = printed_keywords::drain_definition(id, scope, instruction, source);
    PlannedAbility::compile(&Located {
        node,
        source: source.clone(),
    })
}
