use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::optional_payment;
use crate::ir::{
    Bool, Cardinality, ChoiceDomain, Control, Expr, Mode, ModeChoiceKind, ModeDecision,
    ModeDecisionKind, Nat, NatFrozen, NatOperation, Node, PlayerRef, Predicate, Program,
    SpellCountTime, Visibility,
};
use alloc::collections::BTreeSet;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Mode input regions share the structural checker state."
)]
impl Checker {
    pub(super) fn guarded_play_mode(control: &Control) -> bool {
        let Control::If {
            condition,
            then_body,
            else_body: None,
        } = control
        else {
            return false;
        };
        let [
            super::super::Located {
                node:
                    Node::Control {
                        control:
                            Control::Mode {
                                choice_kind: ModeChoiceKind::ChoiceKeyword,
                                ..
                            },
                    },
                ..
            },
        ] = then_body.steps.as_slice()
        else {
            return false;
        };
        optional_payment::public_mode_guard(condition)
    }

    pub(super) fn modes(
        &mut self,
        modes: &[Mode],
        count: &Expr<Nat>,
        chosen: &ModeDecision,
        kind: ModeChoiceKind,
    ) -> Checked {
        if kind == ModeChoiceKind::ChoiceKeyword
            && (!self.env.play_modes || chosen.actor != (PlayerRef::Controller {}))
        {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        if !self.env.choices {
            return self.reject();
        }
        let ChoiceDomain::Modes {
            modes: available,
            cardinality,
            binding,
            exclude_used,
        } = &chosen.domain
        else {
            return Err(self.error(WellFormedReason::InvalidModes));
        };
        let ids: Vec<_> = modes.iter().map(|mode| mode.id).collect();
        if ids.is_empty()
            || ids != *available
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || (exclude_used.is_some() && !exact_play_mode(kind, count, *cardinality))
        {
            return Err(self.error(WellFormedReason::InvalidModes));
        }
        if !self.decisions.insert(chosen.id) {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
        let original = self.env.clone();
        let previous_time = self.count_time;
        let controller = self.play_controller.clone();
        match (kind, chosen.kind) {
            (
                ModeChoiceKind::ChoiceKeyword,
                ModeDecisionKind::PlayParameter { play_scope, input },
            ) => {
                let Some(entry) = &self.play_entry else {
                    return Err(self.error(WellFormedReason::DecisionTiming));
                };
                self.env = entry.clone();
                self.env.scope = original.scope;
                let Some(controller) = &controller else {
                    return Err(self.error(WellFormedReason::DecisionTiming));
                };
                self.play_input(play_scope, input, &chosen.actor, controller)?;
                self.count_time = Some(SpellCountTime::AtPlay);
            }
            (
                ModeChoiceKind::ResolutionAlternative,
                ModeDecisionKind::ResolutionChoice {
                    cardinality: bounds,
                },
            ) if bounds == *cardinality
                && bounds.minimum == Nat(1)
                && bounds.maximum == Some(Nat(1))
                && *count == Expr::Literal(Nat(1)) =>
            {
                self.count_time = Some(SpellCountTime::AtResolution);
            }
            _ => return Err(self.error(WellFormedReason::DecisionTiming)),
        }
        self.player(&chosen.actor)?;
        self.visibility(&chosen.visibility)?;
        if chosen.visibility != (Visibility::Public {}) {
            return self.reject();
        }
        self.cardinality(*cardinality)?;
        self.at("count", |checker| checker.natural(count))?;
        self.mode_count(count, *cardinality, modes.len())?;
        self.define(*binding, BindingKind::Value)?;
        let input = self.env.clone();
        self.count_time = previous_time;
        for (index, mode) in modes.iter().enumerate() {
            self.env = input.clone();
            self.env.scope = mode.body.scope;
            self.env.play_modes = false;
            self.at(&format!("modes/{index}"), |checker| {
                if kind == ModeChoiceKind::ResolutionAlternative && !mode.parameters.is_empty() {
                    return Err(checker.error(WellFormedReason::DecisionTiming));
                }
                for (parameter_index, decision) in mode.parameters.iter().enumerate() {
                    let Some(controller) = &controller else {
                        return Err(checker.error(WellFormedReason::DecisionTiming));
                    };
                    checker.at(&format!("parameters/{parameter_index}"), |checker| {
                        checker.play_decision(decision, controller)
                    })?;
                }
                // Resolution locals may be read by the selected body, never by play-time inputs.
                checker.env.inherit_values(&original);
                checker.at("body", |checker| checker.program(&mode.body))
            })?;
        }
        self.env = original;
        Ok(())
    }
    fn mode_count(&mut self, count: &Expr<Nat>, bounds: Cardinality, number: usize) -> Checked {
        match count {
            Expr::Literal(value) => {
                let cap = value.0.min(
                    u64::try_from(number)
                        .map_err(|_overflow| self.error(WellFormedReason::InvalidModes))?,
                );
                if bounds.maximum != Some(Nat(cap))
                    || (bounds.minimum.0 != cap
                        && bounds.minimum.0 != u64::from(cap > 0)
                        && !(bounds.minimum.0 == 0 && usize::try_from(cap) == Ok(number)))
                {
                    return Err(self.error(WellFormedReason::InvalidModes));
                }
                Ok(())
            }
            Expr::Compute(operation) => {
                if bounds.minimum != Nat(1) || bounds.maximum.is_some() {
                    return Err(self.error(WellFormedReason::InvalidModes));
                }
                self.conditional_count(operation)
            }
            Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => self.reject(),
        }
    }

    fn conditional_count(&mut self, operation: &NatOperation) -> Checked {
        let NatOperation::Conditional {
            condition,
            then_value,
            else_value,
        } = operation
        else {
            return self.reject();
        };
        self.mode_condition(condition)?;
        for value in [then_value, else_value] {
            match value {
                Expr::Literal(_) => {}
                Expr::Compute(child) => self.conditional_count(child)?,
                Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => {
                    return self.reject();
                }
            }
        }
        Ok(())
    }

    fn mode_condition(&mut self, condition: &Expr<Bool>) -> Checked {
        match condition {
            Expr::Literal(_) => Ok(()),
            Expr::Compute(predicate) => match predicate.as_ref() {
                Predicate::CompareNat { left, right, .. } => {
                    for value in [left, right] {
                        if !matches!(value, Expr::Literal(_) | Expr::Frozen(_)) {
                            return self.reject();
                        }
                        if let Expr::Frozen(read) = value
                            && !matches!(
                                read.as_ref(),
                                NatFrozen::SpellChain { .. } | NatFrozen::NecroCharge { .. }
                            )
                        {
                            return self.reject();
                        }
                    }
                    Ok(())
                }
                Predicate::And { predicates } | Predicate::Or { predicates } => {
                    for child in predicates {
                        self.mode_condition(child)?;
                    }
                    Ok(())
                }
                Predicate::Not { predicate } => self.mode_condition(predicate),
                Predicate::SearchFoundAny { .. }
                | Predicate::EventCause { .. }
                | Predicate::Equal { .. }
                | Predicate::PlayerEquals { .. }
                | Predicate::PlayerIn { .. }
                | Predicate::ReceiptStatusEquals { .. }
                | Predicate::PlayedCard { .. }
                | Predicate::PlayedAbility { .. }
                | Predicate::HolderIsPlayer { .. }
                | Predicate::HolderIsObject { .. }
                | Predicate::AnyStatFacet { .. }
                | Predicate::AnySnapshot { .. }
                | Predicate::AnyMove { .. }
                | Predicate::MoveSucceeded { .. }
                | Predicate::ReceiptAny { .. }
                | Predicate::ObjectEquals { .. }
                | Predicate::RecipientIsLeader { .. }
                | Predicate::RecipientIsFollower { .. }
                | Predicate::ZoneEquals { .. }
                | Predicate::PostureEquals { .. }
                | Predicate::EvolutionEquals { .. }
                | Predicate::PhaseEquals { .. }
                | Predicate::TitleEquals { .. }
                | Predicate::HasAbility { .. }
                | Predicate::HasTrait { .. }
                | Predicate::NamesIntersect { .. }
                | Predicate::KindEquals { .. }
                | Predicate::ClassEquals { .. }
                | Predicate::CompareInt { .. }
                | Predicate::HasInfo { .. }
                | Predicate::NameEquals { .. }
                | Predicate::NameContains { .. }
                | Predicate::MemberOf { .. }
                | Predicate::Exists { .. }
                | Predicate::All { .. }
                | Predicate::Any { .. } => self.reject(),
            },
            Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => self.reject(),
        }
    }
}

pub(super) fn play_mode_groups(program: &Program) -> usize {
    program
        .steps
        .iter()
        .map(|step| match &step.node {
            Node::Control {
                control: Control::Sequence { body },
            } => play_mode_groups(body),
            Node::Control {
                control:
                    Control::Mode {
                        choice_kind: ModeChoiceKind::ChoiceKeyword,
                        ..
                    },
            } => 1,
            Node::Instruction { .. }
            | Node::Control { .. }
            | Node::Capture { .. }
            | Node::Decision { .. } => 0,
        })
        .sum()
}

fn exact_play_mode(kind: ModeChoiceKind, count: &Expr<Nat>, cardinality: Cardinality) -> bool {
    kind == ModeChoiceKind::ChoiceKeyword
        && *count == Expr::Literal(Nat(1))
        && cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
}
