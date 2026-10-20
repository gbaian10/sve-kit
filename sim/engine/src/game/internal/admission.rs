use super::{
    Cardinality, ChoiceDomain, Control, DecisionKind, Declinable, InstructionContext, Located,
    Node, Program, Requirements, Result, SourceSpan, Visibility, WellFormedProgram, unsupported,
};
use core::num::NonZeroU32;

pub(super) fn check(program: &WellFormedProgram, source: &SourceSpan) -> Result<()> {
    let located = Located {
        node: program.program().clone(),
        source: source.clone(),
    };
    let requirements = Requirements::derive_program(&located);
    let missing = requirements
        .required
        .iter()
        .filter(|requirement| {
            requirement.capability.version != NonZeroU32::MIN
                || !supported(requirement.capability.code)
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(unsupported(&format!("requirements {missing:?}")));
    }
    check_program(program.program(), source)
}

fn check_program(program: &Program, source: &SourceSpan) -> Result<()> {
    for step in &program.steps {
        super::ordering::check_node(&step.node, source)?;
        match &step.node {
            Node::Control {
                control: Control::Sequence { body },
            } => check_program(body, source)?,
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                check_program(then_body, source)?;
                if let Some(body) = else_body {
                    check_program(body, source)?;
                }
            }
            Node::Instruction { instruction } => {
                super::movement::check_action(&instruction.action)?;
                super::numeric::check_action(&instruction.action)?;
                if instruction.precondition.is_some()
                    || instruction.context != InstructionContext::Effect
                {
                    return Err(unsupported("instruction prerequisite or cost"));
                }
            }
            Node::Decision { decision } => {
                if !matches!(
                    decision.domain,
                    ChoiceDomain::Integer {
                        cardinality: Cardinality {
                            maximum: Some(_),
                            ..
                        },
                        ..
                    }
                ) || !decision.captures.is_empty()
                    || !matches!(decision.visibility, Visibility::Public {})
                    || !matches!(
                        decision.kind,
                        DecisionKind::ResolutionChoice {
                            may: Declinable::Required,
                            precondition: None,
                            ..
                        }
                    )
                {
                    return Err(unsupported("decision metadata"));
                }
            }
            Node::Capture { .. } => {}
            Node::Control { .. } => return Err(unsupported("control")),
        }
    }
    Ok(())
}

// Every enum arm, internal tag and nested requirement is opt-in, including dead branches.
#[expect(
    clippy::too_many_lines,
    reason = "The exact supported derivation codes are reviewed as one closed set."
)]
fn supported(code: &str) -> bool {
    matches!(
        code,
        "shape.Node.Instruction"
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
            | "player.require-one"
            | "internal.InstructionContext.Effect"
            | "shape.Action.Draw"
            | "shape.Action.Look"
            | "action.look"
            | "shape.Action.Order"
            | "action.order"
            | "shape.Position.Top"
            | "shape.Position.Bottom"
            | "shape.Selector.ReceiptMembers"
            | "read.receipt.members"
            | "shape.ObjectReceipt.Look"
            | "internal.MemberProjection.Before"
            | "internal.MemberProjection.After"
            | "shape.Selector.Difference"
            | "selector.difference"
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
            | "shape.Action.Modify"
            | "action.modify"
            | "effect.continuous"
            | "shape.CharacteristicPatch.Numeric"
            | "shape.CharacteristicPatch.Combined"
            | "shape.NumericChange.Add"
            | "numeric.add"
            | "internal.Stat.Attack"
            | "internal.Stat.Life"
            | "shape.DurationEnd.NoTemporalExpiry"
            | "duration.no-expiry"
            | "shape.EffectReference.CurrentInstallation"
            | "receipt.move-after"
            | "shape.MoveDestination.Owners"
            | "internal.OwnerZone.Cemetery"
            | "internal.OwnerZone.Banish"
            | "internal.MoveReason.Put"
            | "shape.Action.InstallEffect"
            | "action.install-effect"
            | "shape.InstallationSubjects.Objects"
            | "internal.InstallationTime.InstructionStart"
            | "shape.Effect.Permission"
            | "effect.permission"
            | "shape.PermissionSubject.Objects"
            | "shape.ActionPattern.Destroy"
            | "shape.CausePattern.Effect"
            | "shape.EffectSource.Any"
            | "internal.Polarity.Forbid"
            | "shape.DurationEnd.EndResolution"
            | "duration.resolution"
            | "shape.TimeAnchor.Installation"
            | "shape.Selector.Bound"
            | "shape.Action.Damage"
            | "action.damage"
            | "internal.DamageFlags.Ability"
            | "shape.ObjectView.Current"
            | "shape.BatchPolicy.Instruction"
            | "shape.DamageAmount.Uniform"
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
            | "receipt.local"
            | "decision.input"
            | "shape.ChoiceDomain.Integer"
            | "shape.DecisionKind.ResolutionChoice"
            | "decision.resolution-choice"
            | "internal.Declinable.Required"
            | "shape.Visibility.Public"
            | "shape.Visibility.Only"
    )
}
