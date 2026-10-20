use super::{
    Arithmetic, Binding, BindingKind, Bool, BoolRead, Cardinality, Checked, Checker, Expr, Holder,
    Int, IntFrozen, IntOperation, IntRead, MeasuredReceipt, MeasuredReceiptList, Nat, NatFrozen,
    NatOperation, NatRead, ObjectOperand, ObjectReceipt, ObjectReceiptList, ObjectRef, ObjectView,
    PlayerRef, PlayerRelation, PlayerSet, Predicate, SelectionView, Selector, WellFormedReason,
};
use crate::ir::{BoolFrozen, PaymentReference};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Control and value rules share one checker state but live in separate modules."
)]
impl Checker {
    pub(super) fn integer(&mut self, expression: &Expr<Int>) -> Checked {
        if self.env.chooser.is_some() && matches!(expression, Expr::Captured(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => self.reference(reference.binding, BindingKind::Capture),
            Expr::Live(read) => match read.as_ref() {
                IntRead::PlayerResource { player, .. } => self.player(player),
                IntRead::HistorySum { .. } => self.reject(),
            },
            Expr::Frozen(read) => match read.as_ref() {
                IntFrozen::ReceiptRequested { receipt } => self.measured(receipt),
                IntFrozen::StatBefore { .. }
                | IntFrozen::StatAfter { .. }
                | IntFrozen::LifeChangeBefore { .. }
                | IntFrozen::LifeChangeAfter { .. }
                | IntFrozen::LifeChangeDelta { .. }
                | IntFrozen::CounterChangeDelta { .. }
                | IntFrozen::DamageAmount { .. }
                | IntFrozen::StatDelta { .. } => self.reject(),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                IntOperation::Arithmetic {
                    operation,
                    arguments,
                } => {
                    // Author min/max take one or more operands; the other operations are fixed.
                    let arity = match operation {
                        Arithmetic::HalfUp => 1..=1,
                        Arithmetic::Minimum | Arithmetic::Maximum => 1..=usize::MAX,
                        Arithmetic::Add
                        | Arithmetic::Subtract
                        | Arithmetic::Multiply
                        | Arithmetic::FloorDivide => 2..=2,
                    };
                    if !arity.contains(&arguments.len()) {
                        return Err(self.error(WellFormedReason::ArithmeticArity));
                    }
                    if *operation == Arithmetic::FloorDivide {
                        match arguments.last() {
                            Some(Expr::Literal(Int(divisor))) if *divisor > 0 => {}
                            Some(Expr::Literal(_)) => {
                                return Err(self.error(WellFormedReason::InvalidDivisor));
                            }
                            _ => return self.reject(),
                        }
                    }
                    for argument in arguments {
                        self.integer(argument)?;
                    }
                    Ok(())
                }
                IntOperation::FromNat { value } => self.natural(value),
                IntOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => {
                    self.boolean(condition)?;
                    self.integer(then_value)?;
                    self.integer(else_value)
                }
                IntOperation::Sum { objects, .. } => self.selector(objects),
                IntOperation::InformationOrZero { .. } => self.reject(),
            },
        }
    }

    pub(super) fn natural(&mut self, expression: &Expr<Nat>) -> Checked {
        if self.env.chooser.is_some() && matches!(expression, Expr::Captured(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => self.reference(reference.binding, BindingKind::Capture),
            Expr::Frozen(read) => match read.as_ref() {
                NatFrozen::ReceiptActual { receipt } => self.measured(receipt),
                NatFrozen::EarthRiteRemovedStack { .. }
                | NatFrozen::DiceAcceptedAttempt { .. }
                | NatFrozen::TargetedTargetSlot { .. }
                | NatFrozen::DrawOrdinal { .. }
                | NatFrozen::CounterChangeBefore { .. }
                | NatFrozen::CounterChangeAfter { .. }
                | NatFrozen::AttackOrdinal { .. }
                | NatFrozen::RaceOrdinal { .. }
                | NatFrozen::SpellChain { .. }
                | NatFrozen::NecroCharge { .. }
                | NatFrozen::DiceValue { .. } => self.reject(),
            },
            Expr::Live(read) => match read.as_ref() {
                NatRead::ZoneCount { selector } => self.selector(selector),
                NatRead::Counter { holder, .. } => self.holder(holder),
                NatRead::ZoneCapacity { player, .. } | NatRead::TurnOrdinal { player } => {
                    self.player(player)
                }
                NatRead::Combo { .. } | NatRead::HistoryCount { .. } => self.reject(),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                NatOperation::ReceiptActualSum { receipts } => match receipts {
                    MeasuredReceiptList::Cards { receipts } => {
                        self.reference(*receipts, BindingKind::Value)
                    }
                    MeasuredReceiptList::Damage { receipts } => {
                        self.reference(*receipts, BindingKind::Value)
                    }
                    MeasuredReceiptList::Counters { receipts } => {
                        self.reference(*receipts, BindingKind::Value)
                    }
                },
                NatOperation::CheckedInt { value } => self.integer(value),
                NatOperation::Count { objects } | NatOperation::DistinctCount { objects, .. } => {
                    self.selector(objects)
                }
                NatOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => {
                    self.boolean(condition)?;
                    self.natural(then_value)?;
                    self.natural(else_value)
                }
            },
        }
    }

    pub(super) fn boolean(&mut self, expression: &Expr<Bool>) -> Checked {
        if self.env.chooser.is_some() && matches!(expression, Expr::Captured(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => self.reference(reference.binding, BindingKind::Capture),
            Expr::Live(read) => match read.as_ref() {
                BoolRead::Awakening { player } | BoolRead::OwnTurn { player } => {
                    self.player(player)
                }
                BoolRead::State { object, .. } => self.view(object),
                BoolRead::HistoryExists { .. } | BoolRead::LegalAttack { .. } => self.reject(),
            },
            Expr::Frozen(read) => match read.as_ref() {
                BoolFrozen::PaymentComplete {
                    receipt: PaymentReference::Local { receipt },
                } => self.receipt(receipt),
                BoolFrozen::PaymentsComplete { receipts, .. } => {
                    self.reference(*receipts, BindingKind::Value)
                }
                BoolFrozen::PaymentZero { .. }
                | BoolFrozen::PaymentReplaced { .. }
                | BoolFrozen::DriveGainedFirstGain { .. }
                | BoolFrozen::DriveCheckedTriggerAccepted { .. }
                | BoolFrozen::PaymentComplete { .. } => self.reject(),
            },
            Expr::Compute(predicate) => self.predicate(predicate),
        }
    }

    fn predicate(&mut self, expression: &Predicate) -> Checked {
        match expression {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                for predicate in predicates {
                    self.boolean(predicate)?;
                }
                Ok(())
            }
            Predicate::Not { predicate } => self.boolean(predicate),
            Predicate::CompareInt { left, right, .. } => {
                self.integer(left)?;
                self.integer(right)
            }
            Predicate::CompareNat { left, right, .. } => {
                self.natural(left)?;
                self.natural(right)
            }
            Predicate::Exists { objects } => self.selector(objects),
            Predicate::MemberOf { object, set } => {
                self.object(object)?;
                self.selector(set)
            }
            Predicate::All {
                objects,
                binding,
                predicate,
            }
            | Predicate::Any {
                objects,
                binding,
                predicate,
            } => {
                self.selector(objects)?;
                self.candidate(*binding, predicate)
            }
            Predicate::EventCause { .. }
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
            | Predicate::HasInfo { .. }
            | Predicate::NameEquals { .. }
            | Predicate::NameContains { .. } => self.reject(),
        }
    }

    fn scalar<T>(&self, binding: Binding<T>) -> Checked
    where
        T: 'static,
    {
        self.reference(binding, BindingKind::Value)?;
        if self.env.chooser.is_some() && !self.env.public_scalars.contains(&binding.key()) {
            return self.reject();
        }
        Ok(())
    }

    pub(super) fn player(&mut self, player: &PlayerRef) -> Checked {
        match player {
            PlayerRef::Controller {} | PlayerRef::Owner {} | PlayerRef::Active {} => Ok(()),
            PlayerRef::Bound { binding } => self.reference(*binding, BindingKind::Value),
            PlayerRef::RequireOne { players } => self.players(players),
            PlayerRef::FromExpression { .. } => self.reject(),
        }
    }

    pub(super) fn players(&mut self, players: &PlayerSet) -> Checked {
        self.cardinality(players.cardinality)?;
        if players.relation == PlayerRelation::Bound {
            return self.reject();
        }
        self.player(&players.relative_to)
    }

    pub(super) fn cardinality(&self, cardinality: Cardinality) -> Checked {
        if cardinality
            .maximum
            .is_some_and(|maximum| maximum.0 < cardinality.minimum.0)
        {
            return Err(self.error(WellFormedReason::InvalidCardinality));
        }
        Ok(())
    }

    pub(super) fn holder(&mut self, holder: &Holder) -> Checked {
        match holder {
            Holder::Object { object } => self.object(object),
            Holder::Player { player } => self.player(player),
            Holder::FromExpression { .. } => self.reject(),
        }
    }

    pub(super) fn object(&self, object: &ObjectOperand) -> Checked {
        match object {
            ObjectOperand::SourceHolder {} => Ok(()),
            ObjectOperand::Bound { binding } => {
                self.reference(*binding, BindingKind::Value)?;
                if self.env.chooser.is_some()
                    && !self.env.choice_candidates.contains(&binding.key())
                {
                    return self.reject();
                }
                Ok(())
            }
            ObjectOperand::FollowEventMove { .. }
            | ObjectOperand::FromExpression { .. }
            | ObjectOperand::Target { .. }
            | ObjectOperand::Moved { .. } => self.reject(),
        }
    }

    fn view(&self, view: &ObjectView) -> Checked {
        match view {
            ObjectView::Current { object } => self.object(object),
            ObjectView::Snapshot { .. }
            | ObjectView::LastKnown { .. }
            | ObjectView::EventSnapshot { .. } => self.reject(),
        }
    }

    pub(super) fn selector(&mut self, selector: &Selector) -> Checked {
        self.selection_visibility(selector)?;
        match selector {
            Selector::Bound { reference } => self.reference(*reference, BindingKind::Value),
            Selector::Captured { reference } => {
                self.reference(reference.binding, BindingKind::Capture)
            }
            Selector::Objects { objects } => {
                for object in objects {
                    self.object(object)?;
                }
                Ok(())
            }
            Selector::Zone {
                players,
                view: SelectionView::Current {},
                binding,
                predicate,
                ..
            } => {
                self.players(players)?;
                self.candidate(*binding, predicate)
            }
            Selector::Filter {
                input,
                binding,
                predicate,
            } => {
                self.selector(input)?;
                self.candidate(*binding, predicate)
            }
            Selector::Union { sets } | Selector::Intersection { sets } => {
                if sets.is_empty() {
                    return self.reject();
                }
                for set in sets {
                    self.selector(set)?;
                }
                Ok(())
            }
            Selector::Difference { input, except } => {
                self.selector(input)?;
                self.selector(except)
            }
            Selector::Ordered { zone, count, .. } => {
                self.player(&zone.player)?;
                self.natural(count)
            }
            Selector::Created { receipt } => self.receipt(receipt),
            Selector::ReceiptMembers { receipt, .. } => match receipt {
                ObjectReceipt::Look { receipt } => self.receipt(receipt),
                ObjectReceipt::Flip { receipt } => self.receipt(receipt),
                ObjectReceipt::Token { receipt } => self.receipt(receipt),
                ObjectReceipt::Batch { receipt } => self.receipt(receipt),
                ObjectReceipt::TransformBanished { .. }
                | ObjectReceipt::TransformCreated { .. }
                | ObjectReceipt::Movement { .. } => self.reject(),
            },
            Selector::CollectedMembers { receipts, .. } => match receipts {
                ObjectReceiptList::Look { receipts } => {
                    self.reference(*receipts, BindingKind::Value)
                }
                ObjectReceiptList::Flip { receipts } => {
                    self.reference(*receipts, BindingKind::Value)
                }
                ObjectReceiptList::Token { receipts } => {
                    self.reference(*receipts, BindingKind::Value)
                }
                ObjectReceiptList::Batch { receipts } => {
                    self.reference(*receipts, BindingKind::Value)
                }
            },
            Selector::MoveAfterAll { .. }
            | Selector::MoveAfter { .. }
            | Selector::Zone { .. }
            | Selector::Relation { .. } => self.reject(),
        }
    }

    fn candidate(&mut self, binding: Binding<ObjectRef>, predicate: &Expr<Bool>) -> Checked {
        self.isolated(|checker| {
            let scope = binding.key().0;
            if scope != checker.env.scope && !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(binding, BindingKind::Value)?;
            if checker.env.chooser.is_some() {
                checker.env.choice_candidates.insert(binding.key());
            }
            checker.boolean(predicate)
        })
    }

    fn measured(&self, receipt: &MeasuredReceipt) -> Checked {
        match receipt {
            MeasuredReceipt::Cards { receipt } => self.receipt(receipt),
            MeasuredReceipt::Damage { receipt } => self.receipt(receipt),
            MeasuredReceipt::Counters { receipt } => self.receipt(receipt),
        }
    }
}
