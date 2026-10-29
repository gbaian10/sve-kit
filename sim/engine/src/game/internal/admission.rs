use super::{
    Cardinality, ChoiceDomain, Control, DecisionKind, Declinable, InstructionContext, Located,
    Node, Program, Requirements, Result, SourceSpan, Visibility, WellFormedProgram, unsupported,
};
use crate::ir::{
    AbilityOrigin, AbilitySource, Binding, CostSpec, Decision, Disjointness, EnterFieldEvent,
    KeywordId, MemberProjection, Nat, ObjectReceipt, PlayerRef, PlayerRelation, Requirement,
    Selector,
};
use crate::ir::{Action, DamageAmount, IntFrozen, IntOperation, Predicate};
use crate::ir::{Bool, DamageEvent, Expr, Int};
use crate::ir::{EventPattern, TriggerCondition, TriggerSpec};
use alloc::collections::BTreeSet;
use core::num::NonZeroU32;

pub(super) fn check(program: &WellFormedProgram, source: &SourceSpan) -> Result<()> {
    check_body(program.program(), source, None, None, None)
}

pub(super) fn check_card(
    program: &Program,
    source: &SourceSpan,
    origin: &AbilitySource,
    trigger: Option<&TriggerSpec>,
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
    check_body(program, source, Some(origin), entry, damage)
}

pub(super) fn check_play(program: &Program, source: &SourceSpan) -> Result<()> {
    let located = Located {
        node: program.clone(),
        source: source.clone(),
    };
    // Play-time reads precede the body's source producer; a Spell is still in Hand or EX here.
    if Requirements::derive_program(&located)
        .required
        .iter()
        .any(|requirement| requirement.capability.code == "reference.source-information")
    {
        return Err(unsupported("play-time source information"));
    }
    check_requirements(program, source, true, &BTreeSet::new())
}

fn check_body(
    program: &Program,
    source: &SourceSpan,
    origin: Option<&AbilitySource>,
    entry: Option<Binding<EnterFieldEvent>>,
    damage: Option<Binding<DamageEvent>>,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    if let Some(origin) = origin {
        source_paths(program, "", origin, &mut paths);
    }
    if let Some(event) = entry {
        super::card::triggers::entry_operand::paths(program, "", event, &mut paths);
    }
    if let Some(event) = damage {
        damage_paths(program, "", event, &mut paths);
    }
    check_requirements(program, source, origin.is_some(), &paths)?;
    super::numeric::check_program(program)?;
    check_program(program, source, origin.is_some())
}

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
                if matches!(
                    origin.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Fanfare
                            | KeywordId::Lastword
                            | KeywordId::OnAttack
                            | KeywordId::OnEvolve
                            | KeywordId::OnSuperEvolve
                            | KeywordId::Quick,
                        ..
                    }
                ) {
                    paths.insert(format!("{path}/instruction/source/origin"));
                    paths.insert(format!("{path}/instruction/source/origin/keyword"));
                }
            }
            Node::Control {
                control:
                    Control::Pay {
                        costs: CostSpec::Ordered { steps },
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
            }
            Node::Control {
                control: Control::Sequence { body },
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
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
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

fn check_requirements(
    program: &Program,
    source: &SourceSpan,
    card: bool,
    origins: &BTreeSet<String>,
) -> Result<()> {
    let located = Located {
        node: program.clone(),
        source: source.clone(),
    };
    let requirements = Requirements::derive_program(&located);
    let missing = requirements
        .required
        .iter()
        .filter(|requirement| {
            requirement.capability.version != NonZeroU32::MIN
                || !(supported(requirement.capability.code)
                    || super::source::count_zone(&requirements, requirement)
                    || super::history_turn::public_requirement(&requirements, requirement)
                    || entry_field_requirement(&requirements, requirement)
                    || (card && play_requirement(requirement.capability.code))
                    || (matches!(
                        requirement.capability.code,
                        "internal.ZoneKind.Evolve"
                            | "shape.AbilityOrigin.Keyword"
                            | "shape.ObjectFrozen.EnterFieldObject"
                            | "shape.IntFrozen.DamageAmount"
                            | "reference.expression"
                            | "shape.ObjectOperand.FromExpression"
                            | "read.event-field"
                            | "internal.KeywordId.Fanfare"
                            | "internal.KeywordId.Lastword"
                            | "internal.KeywordId.OnAttack"
                            | "internal.KeywordId.OnEvolve"
                            | "internal.KeywordId.OnSuperEvolve"
                            | "internal.KeywordId.Quick"
                            | "internal.KeywordId.Ward"
                            | "internal.KeywordId.Aura"
                            | "internal.KeywordId.Storm"
                            | "internal.KeywordId.Rush"
                            | "internal.KeywordId.Bane"
                            | "internal.KeywordId.Intimidate"
                            | "internal.KeywordId.Assail"
                    ) && requirement
                        .locations
                        .iter()
                        .all(|location| origins.contains(&location.ir_path)))
                    || super::search::private_requirement(requirement.capability.code))
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
        "shape.PaymentReference.CardPlay"
            | "internal.OwnerZone.Ex"
            | "shape.ObjectOperand.FollowEventMove"
            | "shape.MovementEvent.LeaveField"
            | "reference.trigger-follow"
            | "read.event-header.move-receipt"
            | "read.card-play.payment-complete"
            | "shape.BoolFrozen.PaymentComplete"
            | "read.receipt.payment-complete"
            | "shape.ObjectView.SourceInformation"
            | "reference.source-information"
            | "shape.NatFrozen.SpellChain"
            | "read.spell-chain"
            | "shape.NatFrozen.NecroCharge"
            | "read.necro-charge"
            | "internal.SpellCountTime.AtPlay"
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
            | "internal.PlaySlot.Targets"
            | "internal.PlaySlot.CostMaterials"
            | "internal.PlaySlot.Distribution"
            | "internal.SelectionSnapshot.AtDecision"
            | "internal.InputMode.Inline"
            | "shape.Control.Mode"
            | "control.mode"
            | "shape.ChoiceDomain.Modes"
            | "shape.ModeDecisionKind.PlayParameter"
            | "internal.ModeChoiceKind.ChoiceKeyword"
            | "internal.ModeExclusion.CurrentTurnAbilityInstance"
    )
}

fn check_action(action: &Action) -> Result<()> {
    super::search::check_action(action)?;
    super::tokens::check_action(action)?;
    super::movement::check_action(action)?;
    super::resource::check_action(action)?;
    super::discard::check_action(action)?;
    super::numeric::check_action(action)?;
    super::grant::check_action(action)
}

fn check_program(program: &Program, source: &SourceSpan, card: bool) -> Result<()> {
    for step in &program.steps {
        super::ordering::check_node(&step.node, source)?;
        match &step.node {
            Node::Control {
                control: control @ Control::Pay { costs, on_paid, .. },
            } if card => {
                super::card::optional::check_control(control)?;
                if !on_paid.steps.is_empty() {
                    return Err(unsupported("optional continuation"));
                }
                let CostSpec::Ordered { steps } = costs else {
                    return Err(unsupported("optional alternatives"));
                };
                if steps.len() != 1 {
                    return Err(unsupported("one optional cost"));
                }
            }
            Node::Control {
                control: Control::Sequence { body },
            } => check_program(body, source, card)?,
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_program(then_body, source, card)?;
                if let Some(body) = else_body {
                    check_program(body, source, card)?;
                }
            }
            Node::Instruction { instruction } => {
                check_action(&instruction.action)?;
                if instruction.precondition.is_some()
                    || instruction.context != InstructionContext::Effect
                {
                    return Err(unsupported("instruction prerequisite or cost"));
                }
            }
            Node::Decision { decision } => {
                if matches!(
                    decision.domain,
                    ChoiceDomain::Objects { .. } | ChoiceDomain::Recipients { .. }
                ) {
                    if !card {
                        return Err(unsupported("resolution field choice outside a whole card"));
                    }
                    super::card::selection::check_resolution(decision)?;
                }
                if matches!(decision.domain, ChoiceDomain::Distribution { .. })
                    && source.card_no != "synthetic"
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
                let optional_private_look = private_look_choice(decision);
                if !matches!(
                    decision.domain,
                    ChoiceDomain::Integer {
                        cardinality: Cardinality {
                            maximum: Some(_),
                            ..
                        },
                        ..
                    } | ChoiceDomain::Distribution { .. }
                        | ChoiceDomain::Objects { .. }
                        | ChoiceDomain::Recipients { .. }
                ) || !decision.captures.is_empty()
                    || !(public_required || optional_private_look)
                {
                    return Err(unsupported("decision metadata"));
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } if card => {
                for mode in modes {
                    check_program(&mode.body, source, card)?;
                }
            }
            Node::Capture { .. } => {}
            Node::Control { .. } => return Err(unsupported("control")),
        }
    }
    Ok(())
}

fn entry_field_requirement(requirements: &Requirements, requirement: &Requirement) -> bool {
    requirement.capability.code == "read.event-field"
        && requirement.locations.iter().all(|location| {
            requirements.required.iter().any(|other| {
                matches!(
                    other.capability.code,
                    "shape.EntryMethodFields.EnterFieldMethod"
                        | "shape.EntryOriginFields.EnterFieldFrom"
                ) && other
                    .locations
                    .iter()
                    .any(|at| at.ir_path == location.ir_path)
            })
        })
}

fn private_look_choice(decision: &Decision) -> bool {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return false;
    };
    if constraint.groups.is_empty() || constraint.groups.len() > 2 {
        return false;
    }
    let maximum = u64::try_from(constraint.groups.len()).ok();
    let expected_bounds = Cardinality {
        minimum: Nat(0),
        maximum: maximum.map(Nat),
    };
    constraint.groups.iter().all(|group| {
        private_look_selector(&group.selector)
            && group.cardinality
                == (Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(1)),
                })
    }) && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && constraint.disjoint == Disjointness::Disjoint
        && matches!(
            &decision.visibility,
            Visibility::Only { players }
                if players.relation == PlayerRelation::SelfPlayer
                    && players.relative_to == decision.actor
                    && decision.actor == PlayerRef::Controller {}
        )
        && matches!(
            decision.kind,
            DecisionKind::ResolutionChoice {
                may: Declinable::MayDecline,
                cardinality,
                precondition: None,
            } if cardinality == expected_bounds
        )
        && constraint.cardinality == expected_bounds
}

fn private_look_selector(selector: &Selector) -> bool {
    matches!(
        selector,
        Selector::Filter { input, .. }
            if matches!(input.as_ref(), Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { .. },
                projection: MemberProjection::Before,
            })
    )
}

// Every enum arm, internal tag and nested requirement is opt-in, including dead branches.
#[expect(
    clippy::too_many_lines,
    reason = "The exact supported derivation codes are reviewed as one closed set."
)]
fn supported(code: &str) -> bool {
    matches!(
        code,
        "shape.Control.Pay"
            | "control.pay"
            | "internal.PaymentTime.PlayParameter"
            | "internal.PaymentOptionality.Optional"
            | "internal.PaymentOptionality.Required"
            | "shape.CostSpec.Ordered"
            | "shape.CostStep"
            | "internal.InstructionContext.Cost"
            | "internal.ResourceOperation.Pay"
            | "shape.PaymentReference.Local"
            | "read.receipt.payment-complete"
            | "shape.Node.Instruction"
            | "shape.Node.Control"
            | "shape.Node.Capture"
            | "shape.Node.Decision"
            | "shape.Control.Sequence"
            | "shape.Control.If"
            | "control.sequence"
            | "control.if"
            | "shape.AbilityOrigin.Printed"
            | "shape.ObjectOperand.SourceHolder"
            | "reference.object-generation"
            | "shape.PlayerRef.Controller"
            | "shape.PlayerRef.Owner"
            | "shape.PlayerRef.Active"
            | "shape.PlayerRef.RequireOne"
            | "shape.PlayerRef.FromExpression"
            | "shape.Capture.Player"
            | "shape.PlayerRead.Role"
            | "shape.ObjectView.Selected"
            | "reference.selected"
            | "read.roles"
            | "internal.Role.Owner"
            | "internal.Role.Controller"
            | "player.require-one"
            | "internal.InstructionContext.Effect"
            | "shape.Action.Draw"
            | "shape.Action.Token"
            | "action.token"
            | "shape.TokenDestination.Single"
            | "shape.Action.Resource"
            | "action.resource"
            | "internal.Resource.PpCurrent"
            | "internal.Resource.PpMaximum"
            | "internal.Resource.Ep"
            | "internal.ResourceOperation.Add"
            | "shape.Action.Discard"
            | "action.discard"
            | "shape.DiscardMode.Chosen"
            | "discard.chosen"
            | "shape.DiscardMode.All"
            | "discard.all"
            | "internal.ZoneKind.Hand"
            | "shape.Action.Reveal"
            | "action.reveal"
            | "shape.Action.Look"
            | "action.look"
            | "shape.Action.Order"
            | "action.order"
            | "shape.Position.Top"
            | "shape.Position.Bottom"
            | "shape.Selector.ReceiptMembers"
            | "read.receipt.members"
            | "shape.ObjectReceipt.Look"
            | "shape.ObjectReceipt.Movement"
            | "shape.MovementReceipt.Move"
            | "shape.MovementReceipt.Search"
            | "shape.MovementReceipt.Destroy"
            | "shape.MovementReceipt.Banish"
            | "shape.Selector.MoveAfter"
            | "reference.follow-generation"
            | "shape.FollowPermission.SameEffectExplicitReference"
            | "internal.MemberProjection.Before"
            | "internal.MemberProjection.After"
            | "shape.Selector.Difference"
            | "selector.difference"
            | "shape.Selector.Filter"
            | "selector.filter"
            | "internal.OwnerZone.Deck"
            | "internal.ZoneKind.Deck"
            | "action.draw"
            | "internal.StepwiseMode.Exact"
            | "internal.StepwiseMode.UpToStepwise"
            | "internal.StepwiseTiming.BeforeDraw"
            | "shape.Action.Move"
            | "action.move"
            | "shape.Action.Destroy"
            | "action.destroy"
            | "shape.Action.Life"
            | "action.life"
            | "shape.Action.Banish"
            | "action.banish"
            | "shape.Action.Posture"
            | "action.posture"
            | "internal.Posture.Acted"
            | "internal.Posture.Standing"
            | "shape.Action.Grant"
            | "action.grant"
            | "shape.GrantedAbility.Keyword"
            | "shape.Action.Modify"
            | "action.modify"
            | "effect.continuous"
            | "shape.CharacteristicPatch.Numeric"
            | "shape.CharacteristicPatch.Combined"
            | "shape.NumericChange.Add"
            | "numeric.add"
            | "shape.NumericChange.Set"
            | "numeric.set"
            | "internal.Stat.Attack"
            | "internal.Stat.Life"
            | "shape.DurationEnd.NoTemporalExpiry"
            | "duration.no-expiry"
            | "shape.EffectReference.CurrentInstallation"
            | "receipt.move-after"
            | "shape.MoveDestination.Owners"
            | "internal.OwnerZone.Cemetery"
            | "internal.OwnerZone.Banish"
            | "internal.OwnerZone.Hand"
            | "internal.MoveReason.Put"
            | "shape.Action.InstallEffect"
            | "action.install-effect"
            | "shape.InstallationSubjects.Objects"
            | "internal.InstallationTime.InstructionStart"
            | "shape.Effect.Permission"
            | "shape.Effect.Continuous"
            | "shape.EffectTarget.Dynamic"
            | "shape.TimestampBasis.ProperPlay"
            | "shape.ExecutionRef.Current"
            | "internal.ContinuousLayer.Numeric"
            | "shape.DurationEnd.SourceZoneExit"
            | "duration.source-zone-exit"
            | "shape.Interval.DuringNextTurn"
            | "duration.next-turn-interval"
            | "shape.Effect.CostModifier"
            | "effect.cost-modifier"
            | "shape.CostChange.Delta"
            | "effect.cost-delta"
            | "shape.NumberRead.Cost"
            | "read.cost"
            | "effect.permission"
            | "shape.PermissionSubject.Objects"
            | "shape.ActionPattern.Destroy"
            | "shape.CausePattern.Effect"
            | "shape.EffectSource.Any"
            | "internal.Polarity.Forbid"
            | "shape.DurationEnd.EndResolution"
            | "duration.resolution"
            | "shape.DurationEnd.CurrentTurnEnd"
            | "duration.turn"
            | "shape.DurationEnd.NextTurnEnd"
            | "duration.next-turn-end"
            | "shape.TimeAnchor.Installation"
            | "shape.Selector.Bound"
            | "shape.Action.Damage"
            | "action.damage"
            | "internal.DamageFlags.Ability"
            | "shape.ObjectView.Current"
            | "shape.BatchPolicy.Instruction"
            | "shape.DamageAmount.Uniform"
            | "shape.DamageAmount.Allocation"
            | "damage.allocation"
            | "shape.RecipientSet.Objects"
            | "recipient.objects"
            | "shape.RecipientSet.Leaders"
            | "recipient.leaders"
            | "shape.RecipientSet.Union"
            | "recipient.union"
            | "shape.RecipientSet.Explicit"
            | "recipient.explicit"
            | "shape.DamageRecipient.Object"
            | "shape.DamageRecipient.Leader"
            | "shape.Selector.Objects"
            | "selector.objects"
            | "shape.Selector.Ordered"
            | "selector.ordered"
            | "internal.SliceDirection.Forward"
            | "shape.Selector.Zone"
            | "selector.zone"
            | "shape.Selector.Union"
            | "selector.union"
            | "shape.Selector.Captured"
            | "shape.SelectionView.Current"
            | "value.live"
            | "internal.ZoneKind.Field"
            | "internal.ZoneKind.Ex"
            | "internal.ZoneKind.Cemetery"
            | "internal.ZoneKind.Banish"
            | "internal.PlayerRelation.SelfPlayer"
            | "internal.PlayerRelation.Opponents"
            | "internal.PlayerRelation.All"
            | "internal.TextQuantifier.Implicit"
            | "internal.TextQuantifier.All"
            | "internal.TextQuantifier.One"
            | "cardinality.bounded"
            | "cardinality.unbounded"
            | "shape.Expr.Literal"
            | "shape.Expr.Bound"
            | "shape.Expr.Captured"
            | "shape.Expr.Compute"
            | "shape.Expr.Live"
            | "shape.Expr.Frozen"
            | "value.signed-literal"
            | "value.capture"
            | "shape.CaptureTime.AtResolution"
            | "shape.Capture.Integer"
            | "shape.Capture.Natural"
            | "shape.Capture.Boolean"
            | "shape.Capture.Objects"
            | "shape.IntRead.PlayerResource"
            | "shape.BoolRead.Crimson"
            | "read.crimson"
            | "shape.NatRead.Combo"
            | "read.combo"
            | "shape.NatRead.HistoryCount"
            | "read.history.count"
            | "shape.EventPattern.Play"
            | "event.match.Play"
            | "shape.HistoryRange.CurrentTurn"
            | "shape.HistoryCutoff.CurrentPoint"
            | "shape.InclusionPolicy.IncludeAll"
            | "internal.HistoryCount.Facts"
            | "read.player-resource"
            | "internal.PlayerResource.Life"
            | "internal.PlayerResource.PpCurrent"
            | "internal.PlayerResource.PpMaximum"
            | "internal.PlayerResource.Ep"
            | "internal.PlayerResource.Sep"
            | "shape.IntOperation.FromNat"
            | "shape.IntOperation.InformationOrZero"
            | "value.absent-number-zero"
            | "shape.NumberRead.Stats"
            | "read.stats"
            | "internal.CharacteristicView.Effective"
            | "value.nat-to-int"
            | "shape.IntOperation.Conditional"
            | "value.conditional"
            | "shape.IntOperation.Arithmetic"
            | "value.signed-arithmetic"
            | "internal.Arithmetic.Add"
            | "internal.Arithmetic.Subtract"
            | "internal.Arithmetic.Multiply"
            | "internal.Arithmetic.Minimum"
            | "internal.Arithmetic.Maximum"
            | "shape.NatOperation.Count"
            | "value.aggregate.count"
            | "shape.NatOperation.CheckedInt"
            | "value.checked-nat"
            | "shape.NatOperation.Conditional"
            | "shape.NatRead.ZoneCount"
            | "read.zone-count"
            | "shape.Predicate.And"
            | "predicate.and"
            | "shape.Predicate.Or"
            | "predicate.or"
            | "shape.Predicate.Not"
            | "predicate.not"
            | "shape.Predicate.CompareInt"
            | "predicate.compare-int"
            | "shape.Predicate.CompareNat"
            | "predicate.compare-nat"
            | "shape.Predicate.Equal"
            | "predicate.equal"
            | "shape.ValueEquality.EntryMethod"
            | "shape.ValueEquality.EntryOrigin"
            | "shape.EntryOrigin.Zone"
            | "shape.EntryOriginFields.EnterFieldFrom"
            | "shape.EntryMethodFields.EnterFieldMethod"
            | "internal.EntryMethod.Play"
            | "internal.EntryMethod.AbilityPut"
            | "internal.EntryMethod.RulePut"
            | "shape.Predicate.Exists"
            | "predicate.exists"
            | "internal.Comparison.Equal"
            | "internal.Comparison.NotEqual"
            | "internal.Comparison.Less"
            | "internal.Comparison.LessEqual"
            | "internal.Comparison.Greater"
            | "internal.Comparison.GreaterEqual"
            | "shape.IntFrozen.ReceiptRequested"
            | "read.receipt.requested"
            | "shape.NatFrozen.ReceiptActual"
            | "read.receipt.actual"
            | "shape.MeasuredReceipt.Cards"
            | "shape.MeasuredReceipt.Damage"
            | "receipt.local"
            | "decision.input"
            | "shape.ChoiceDomain.Integer"
            | "shape.ChoiceDomain.Distribution"
            | "shape.DecisionKind.ResolutionChoice"
            | "decision.resolution-choice"
            | "internal.Declinable.Required"
            | "internal.Declinable.MayDecline"
            | "shape.Visibility.Public"
            | "shape.Visibility.Only"
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
                control: Control::Sequence { body },
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
