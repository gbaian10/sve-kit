use super::{
    Arithmetic, Binding, BindingKind, Bool, BoolRead, Cardinality, Checked, Checker, Expr, Holder,
    Int, IntFrozen, IntOperation, IntRead, MeasuredReceipt, MeasuredReceiptList, Nat, NatFrozen,
    NatOperation, NatRead, ObjectOperand, ObjectReceipt, ObjectReceiptList, ObjectRef, ObjectView,
    PlayerRef, PlayerRelation, PlayerSet, Predicate, SelectionView, Selector, WellFormedReason,
};
use crate::ir::{
    BoolFrozen, CaptureTime, CharacteristicView, ExecutionRef, NumberRead, PaymentReference,
};
use crate::ir::{
    CardKind, CardName, ClassRead, EventSnapshotPolicy, Info, KindRead, MemberProjection,
    NamesRead, ObjectFrozen, TraitsRead,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Control and value rules share one checker state but live in separate modules."
)]
impl Checker {
    pub(super) fn integer(&mut self, expression: &Expr<Int>) -> Checked {
        if self.event_capture() && matches!(expression, Expr::Live(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => {
                self.reference(reference.binding, BindingKind::Capture)?;
                self.visible(self.env.access.get(&reference.binding.key()))
            }
            Expr::Live(read) => match read.as_ref() {
                IntRead::PlayerResource { player, .. } => self.player(player),
                IntRead::HistorySum { query } => self.numeric_history(query),
            },
            Expr::Frozen(read) => match read.as_ref() {
                IntFrozen::ReceiptRequested { receipt } => self.measured(receipt),
                IntFrozen::StatBefore { facet } | IntFrozen::StatAfter { facet } => {
                    self.event_field(*facet)
                }
                IntFrozen::LifeChangeBefore { event }
                | IntFrozen::LifeChangeAfter { event }
                | IntFrozen::LifeChangeDelta { event } => self.event_field(*event),
                IntFrozen::CounterChangeDelta { event } => self.event_field(*event),
                IntFrozen::DamageAmount { event } => self.event_field(*event),
                IntFrozen::StatDelta { event } => self.event_field(*event),
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
                IntOperation::InformationOrZero { information } => match information {
                    NumberRead::Cost { object, view } | NumberRead::Stats { object, view, .. } => {
                        if !matches!(
                            view,
                            CharacteristicView::Original | CharacteristicView::Effective
                        ) {
                            return self.reject();
                        }
                        self.information_view(object)
                    }
                },
            },
        }
    }

    pub(super) fn natural(&mut self, expression: &Expr<Nat>) -> Checked {
        if self.event_capture() && matches!(expression, Expr::Live(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => {
                self.reference(reference.binding, BindingKind::Capture)?;
                self.visible(self.env.access.get(&reference.binding.key()))
            }
            Expr::Frozen(read) => match read.as_ref() {
                NatFrozen::SnapshotCounter { object, .. } => self.projection(object),
                NatFrozen::ReceiptActual { receipt } => self.measured(receipt),
                NatFrozen::SearchFoundCount { found } => self.scalar(*found),
                NatFrozen::SearchRevealedCount { revealed } => self.scalar(*revealed),
                NatFrozen::SpellChain {
                    player,
                    execution,
                    at,
                }
                | NatFrozen::NecroCharge {
                    player,
                    execution,
                    at,
                } => {
                    if self.play_entry.is_none()
                        || *execution != (ExecutionRef::Current {})
                        || self.count_time != Some(*at)
                    {
                        return Err(self.error(WellFormedReason::CaptureTiming));
                    }
                    self.player(player)
                }
                NatFrozen::EarthRiteRemovedStack { component } => self.event_field(*component),
                NatFrozen::DiceAcceptedAttempt { event } | NatFrozen::DiceValue { event } => {
                    self.event_field(*event)
                }
                NatFrozen::TargetedTargetSlot { event } => self.event_field(*event),
                NatFrozen::DrawOrdinal { event } => self.event_field(*event),
                NatFrozen::CounterChangeBefore { event }
                | NatFrozen::CounterChangeAfter { event } => self.event_field(*event),
                NatFrozen::AttackOrdinal { event } => self.event_field(*event),
                NatFrozen::RaceOrdinal { event } => self.event_field(*event),
            },
            Expr::Live(read) => match read.as_ref() {
                NatRead::ZoneCount { selector } => self.selector(selector),
                NatRead::Counter { holder, .. } => self.holder(holder),
                NatRead::ZoneCapacity { player, .. } | NatRead::TurnOrdinal { player } => {
                    self.player(player)
                }
                NatRead::HistoryCount { query, .. } => self.history(query),
                NatRead::Combo { player, execution } => {
                    if *execution != (ExecutionRef::Current {}) {
                        return self.reject();
                    }
                    self.player(player)
                }
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
        if self.event_capture() && matches!(expression, Expr::Live(_)) {
            return self.reject();
        }
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => self.scalar(*binding),
            Expr::Captured(reference) => {
                self.reference(reference.binding, BindingKind::Capture)?;
                self.visible(self.env.access.get(&reference.binding.key()))
            }
            Expr::Live(read) => match read.as_ref() {
                BoolRead::Awakening { player }
                | BoolRead::Crimson { player }
                | BoolRead::OwnTurn { player } => self.player(player),
                BoolRead::State { object, .. } => self.view(object),
                BoolRead::HistoryExists { query } => self.history(query),
                BoolRead::LegalAttack { .. } => self.reject(),
            },
            Expr::Frozen(read) => match read.as_ref() {
                BoolFrozen::PaymentComplete {
                    receipt: PaymentReference::Local { receipt },
                } => self.receipt(receipt),
                BoolFrozen::PaymentsComplete { receipts, .. } => {
                    self.reference(*receipts, BindingKind::Value)
                }
                BoolFrozen::PaymentZero { event } | BoolFrozen::PaymentReplaced { event } => {
                    self.event_field(*event)
                }
                BoolFrozen::DriveGainedFirstGain { event } => self.event_field(*event),
                BoolFrozen::DriveCheckedTriggerAccepted { event } => self.event_field(*event),
                BoolFrozen::PaymentComplete {
                    receipt: PaymentReference::CardPlay { face, receipt },
                } if self.capture_time.is_none()
                    && self.count_time.is_none()
                    && self.env.chooser.is_none()
                    && self.card_payment_body.as_ref() == Some(face)
                    && self.card_payments.get(&face.0) == Some(receipt) =>
                {
                    Ok(())
                }
                BoolFrozen::PaymentComplete { .. } => self.reject(),
            },
            Expr::Compute(predicate) => self.predicate(predicate),
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only explicitly supported shapes pass this gate."
    )]
    fn predicate(&mut self, expression: &Predicate) -> Checked {
        match expression {
            Predicate::ClassEquals {
                left: Expr::Live(read),
                right: Expr::Literal(Info::Present(_)),
            } if !self.event_capture() => {
                let ClassRead::Class { object, view } = read.as_ref();
                if !matches!(
                    view,
                    CharacteristicView::Original | CharacteristicView::Effective
                ) {
                    return self.reject();
                }
                self.view(object)
            }
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
            Predicate::KindEquals { left, right } => {
                self.card_kind(left)?;
                self.card_kind(right)
            }
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
            Predicate::HasTrait { traits, .. } => match traits {
                Expr::Literal(_) => Ok(()),
                Expr::Live(read) if !self.event_capture() => {
                    let TraitsRead::Traits { object, view } = read.as_ref();
                    if !matches!(
                        view,
                        CharacteristicView::Original | CharacteristicView::Effective
                    ) {
                        return self.reject();
                    }
                    self.view(object)
                }
                Expr::Frozen(_) => self.projection(traits),
                Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) | Expr::Compute(_) => {
                    self.reject()
                }
            },
            Predicate::NameContains {
                object,
                name: Expr::Literal(_),
            } => self.view(object),
            Predicate::NamesIntersect { left, right } => {
                self.names(left)?;
                self.names(right)
            }
            Predicate::SearchFoundAny {
                found,
                object,
                predicate,
            } => {
                self.reference(*found, BindingKind::Value)?;
                self.visible_object(self.env.access.get(&found.key()))?;
                self.candidate(*object, predicate)
            }
            Predicate::EvolutionEquals { .. }
            | Predicate::RecipientIsFollower { .. }
            | Predicate::PhaseEquals { .. }
            | Predicate::ZoneEquals { .. }
            | Predicate::EventCause { .. }
            | Predicate::Equal { .. }
            | Predicate::PlayerEquals { .. }
            | Predicate::PlayerIn { .. }
            | Predicate::AnyStatFacet { .. }
            | Predicate::AnySnapshot { .. }
            | Predicate::AnyMove { .. }
            | Predicate::MoveSucceeded { .. }
            | Predicate::ObjectEquals { .. } => self.event_predicate(expression),
            _ => self.reject(),
        }
    }

    fn names(&self, expression: &Expr<Vec<CardName>>) -> Checked {
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Live(read) if !self.event_capture() => {
                let NamesRead::Names { object } = read.as_ref();
                self.view(object)
            }
            Expr::Frozen(_) => self.projection(expression),
            Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) | Expr::Compute(_) => self.reject(),
        }
    }

    fn card_kind(&self, value: &Expr<CardKind>) -> Checked {
        match value {
            Expr::Literal(_) => Ok(()),
            Expr::Live(read) if !self.event_capture() => {
                let KindRead::Kind { object, view } = read.as_ref();
                if !matches!(
                    view,
                    CharacteristicView::Original | CharacteristicView::Effective
                ) {
                    return self.reject();
                }
                self.view(object)
            }
            Expr::Frozen(_) => self.projection(value),
            Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) | Expr::Compute(_) => self.reject(),
        }
    }

    fn scalar<T>(&self, binding: Binding<T>) -> Checked
    where
        T: 'static,
    {
        self.reference(binding, BindingKind::Value)?;
        self.visible(self.env.access.get(&binding.key()))
    }

    pub(super) fn player(&mut self, player: &PlayerRef) -> Checked {
        match player {
            PlayerRef::Controller {} | PlayerRef::Owner {} | PlayerRef::Active {} => Ok(()),
            PlayerRef::Bound { binding } => self.reference(*binding, BindingKind::Value),
            PlayerRef::RequireOne { players } => self.players(players),
            // A Live role can change mid-resolution, so it cannot name a stable audience or actor.
            PlayerRef::FromExpression { value } if matches!(**value, Expr::Live(_)) => {
                self.reject()
            }
            PlayerRef::FromExpression { value } => self.player_value(value),
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
                    self.visible_object(self.env.access.get(&binding.key()))?;
                }
                Ok(())
            }
            ObjectOperand::FollowEventMove { event } => self.follow_event(*event),
            ObjectOperand::FromExpression { value }
                if self.env.trigger == Some(EventSnapshotPolicy::AfterEntry)
                    && matches!(value.as_ref(), Expr::Frozen(field) if matches!(field.as_ref(), ObjectFrozen::EnterFieldObject { .. })) =>
            {
                self.projection(value)
            }
            ObjectOperand::FromExpression { .. }
            | ObjectOperand::Target { .. }
            | ObjectOperand::Moved { .. } => self.reject(),
        }
    }

    pub(super) fn information_view(&self, view: &ObjectView) -> Checked {
        if matches!(view, ObjectView::SourceInformation {}) {
            if self.source_information_entry && !self.event_capture() {
                Ok(())
            } else {
                self.reject()
            }
        } else {
            self.view(view)
        }
    }

    pub(super) fn view(&self, view: &ObjectView) -> Checked {
        match view {
            ObjectView::Selected { selection }
                if matches!(self.capture_time, Some(CaptureTime::AtSelection { .. })) =>
            {
                self.reference(*selection, BindingKind::Value)?;
                self.selection_visibility(&Selector::Bound {
                    reference: *selection,
                })
            }
            ObjectView::Snapshot { value } => self.projection(value),
            ObjectView::Current { object } if !self.event_capture() => self.object(object),
            ObjectView::EventSnapshot {
                object,
                event,
                policy,
            } if self.capture_time
                == Some(CaptureTime::AtEvent {
                    event: *event,
                    policy: *policy,
                }) =>
            {
                self.object(object)
            }
            ObjectView::SourceInformation {}
            | ObjectView::Selected { .. }
            | ObjectView::Current { .. }
            | ObjectView::LastKnown { .. }
            | ObjectView::EventSnapshot { .. } => self.reject(),
        }
    }

    pub(super) fn selector(&mut self, selector: &Selector) -> Checked {
        // This slice has object snapshots, but no event-scoped collection read.
        if self.event_capture() {
            return self.reject();
        }
        self.selection_visibility(selector)?;
        match selector {
            Selector::SearchFound { found } => self.reference(*found, BindingKind::Value),
            Selector::SearchRevealed { revealed } => self.reference(*revealed, BindingKind::Value),
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
            Selector::ReceiptMembers {
                receipt,
                projection,
            } => self.selector_receipt(receipt, *projection),
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
            Selector::MoveAfter {
                receipt,
                permission,
            } => {
                self.follow_permission(*permission)?;
                self.movement_receipt(receipt)
            }
            Selector::MoveAfterAll {
                receipts,
                permission,
                ..
            } => {
                self.follow_permission(*permission)?;
                self.movement_receipts(*receipts)
            }
            Selector::Zone { .. } | Selector::Relation { .. } => self.reject(),
        }
    }

    fn selector_receipt(&self, receipt: &ObjectReceipt, projection: MemberProjection) -> Checked {
        match receipt {
            ObjectReceipt::Look { receipt } => self.receipt(receipt),
            ObjectReceipt::Flip { receipt } => self.receipt(receipt),
            ObjectReceipt::Token { receipt } => self.receipt(receipt),
            ObjectReceipt::Batch { receipt } => self.receipt(receipt),
            ObjectReceipt::Movement { receipt } => {
                if projection != MemberProjection::Before {
                    return self.reject();
                }
                self.movement_receipt(receipt)
            }
            ObjectReceipt::TransformBanished { .. } | ObjectReceipt::TransformCreated { .. } => {
                self.reject()
            }
        }
    }

    pub(super) const fn event_capture(&self) -> bool {
        matches!(self.capture_time, Some(CaptureTime::AtEvent { .. }))
    }

    pub(super) fn candidate(
        &mut self,
        binding: Binding<ObjectRef>,
        predicate: &Expr<Bool>,
    ) -> Checked {
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
