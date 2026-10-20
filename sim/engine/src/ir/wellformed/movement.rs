use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::{
    Binding, Bool, Cardinality, CostChange, Duration, DurationEnd, Effect, Expr, FollowPermission,
    InstallationSubjects, InstallationTime, MoveDestination, MoveReason, MoveSpec, MovementReceipt,
    MovementReceiptList, ObjectRef, OwnerZone, PlayerRef, SearchContinuation, SearchDestination,
    SelectionConstraint, Selector, TimeAnchor, ZoneKind,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Movement checks share the closed environment."
)]
impl Checker {
    pub(super) fn destination(&mut self, destination: &MoveDestination) -> Checked {
        match destination {
            MoveDestination::Owners { .. } => Ok(()),
            MoveDestination::Shared { zone }
                if matches!(zone.kind, ZoneKind::Field | ZoneKind::Ex) =>
            {
                self.player(&zone.player)
            }
            MoveDestination::Shared { .. } => Err(self.error(WellFormedReason::InvalidMovement)),
        }
    }

    pub(super) fn movement(&mut self, movement: &MoveSpec, reason: Option<MoveReason>) -> Checked {
        self.at("to", |checker| checker.destination(&movement.to))?;
        if movement.position.is_some()
            && !matches!(
                movement.to,
                MoveDestination::Owners {
                    zone: OwnerZone::Deck
                }
            )
            || movement.initial_posture.is_some()
                && !matches!(&movement.to, MoveDestination::Shared { zone } if zone.kind == ZoneKind::Field)
            || reason == Some(MoveReason::Mill)
                && movement.to
                    != (MoveDestination::Owners {
                        zone: OwnerZone::Cemetery,
                    })
        {
            return Err(self.error(WellFormedReason::InvalidMovement));
        }
        // Entry installation needs a new-generation subject; a source-holder substitution is unsafe.
        if !movement.entry_effects.is_empty() || movement.order.is_some() {
            return self.reject();
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Borrowed action fields keep filter and continuation regions explicit without cloning the action."
    )]
    pub(super) fn search(
        &mut self,
        player: &PlayerRef,
        deck: &PlayerRef,
        candidate: Binding<ObjectRef>,
        filter: &Expr<Bool>,
        cardinality: Cardinality,
        groups: Option<&SelectionConstraint>,
        destination: Option<&SearchDestination>,
        continuation: SearchContinuation,
    ) -> Checked {
        if !self.env.choices || groups.is_some() {
            return self.reject();
        }
        self.player(player)?;
        self.player(deck)?;
        self.cardinality(cardinality)?;
        self.isolated(|checker| {
            checker.env.chooser = Some(player.clone());
            checker.env.private_choice = true;
            checker.at("filter", |checker| checker.candidate(candidate, filter))
        })?;
        self.isolated(|checker| {
            checker.env.chooser = Some(player.clone());
            checker.env.private_choice = true;
            let scope = continuation.found.key().0;
            if scope == checker.env.scope || !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(continuation.found, BindingKind::Value)?;
            checker.define(continuation.revealed, BindingKind::Value)?;
            checker.at("destination", |checker| match destination {
                None => checker.reject(),
                Some(SearchDestination::Single { movement }) => checker.movement(movement, None),
                Some(SearchDestination::OptionalSelfReplacement {
                    predicate,
                    movement,
                }) => {
                    checker.boolean(predicate)?;
                    checker.movement(movement, None)
                }
                Some(SearchDestination::Partition { destinations }) => {
                    if destinations.is_empty() {
                        return checker.reject();
                    }
                    for (index, partition) in destinations.iter().enumerate() {
                        checker.at(&index.to_string(), |checker| {
                            checker.cardinality(partition.cardinality)?;
                            checker.destination(&partition.zone)
                        })?;
                    }
                    let capacity = destinations.iter().try_fold(0_u64, |total, partition| {
                        partition
                            .cardinality
                            .maximum
                            .map(|maximum| total.saturating_add(maximum.0))
                    });
                    if let Some(capacity) = capacity
                        && cardinality
                            .maximum
                            .is_none_or(|maximum| maximum.0 > capacity)
                    {
                        return checker.reject();
                    }
                    Ok(())
                }
            })
        })
    }

    pub(super) fn follow_permission(&self, permission: FollowPermission) -> Checked {
        match permission {
            FollowPermission::SameEffectExplicitReference {} => Ok(()),
            FollowPermission::TriggerExplicitReference { .. }
            | FollowPermission::RuleCarry { .. } => self.reject(),
        }
    }

    pub(super) fn movement_receipt(&self, receipt: &MovementReceipt) -> Checked {
        match receipt {
            MovementReceipt::Move { receipt } => self.receipt(receipt),
            MovementReceipt::Search { receipt } => self.receipt(receipt),
            MovementReceipt::Discard { receipt } => self.receipt(receipt),
            MovementReceipt::Destroy { receipt } => self.receipt(receipt),
            MovementReceipt::Banish { receipt } => self.receipt(receipt),
            MovementReceipt::Order { receipt } => self.receipt(receipt),
            MovementReceipt::Transform { receipt } => self.receipt(receipt),
            MovementReceipt::Fusion { receipt } => self.receipt(receipt),
            MovementReceipt::TriggerEvent { .. } => self.reject(),
        }
    }

    pub(super) fn movement_receipts(&self, receipts: MovementReceiptList) -> Checked {
        match receipts {
            MovementReceiptList::Move { receipts } => self.reference(receipts, BindingKind::Value),
            MovementReceiptList::Search { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
            MovementReceiptList::Discard { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
            MovementReceiptList::Destroy { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
            MovementReceiptList::Banish { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
            MovementReceiptList::Order { receipts } => self.reference(receipts, BindingKind::Value),
            MovementReceiptList::Transform { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
            MovementReceiptList::Fusion { receipts } => {
                self.reference(receipts, BindingKind::Value)
            }
        }
    }

    pub(super) fn movement_visible(&self, receipt: &MovementReceipt) -> Checked {
        match receipt {
            MovementReceipt::Move { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Search { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Discard { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Destroy { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Banish { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Order { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Transform { receipt } => self.visible_receipt(receipt),
            MovementReceipt::Fusion { receipt } => self.visible_receipt(receipt),
            MovementReceipt::TriggerEvent { .. } => self.reject(),
        }
    }

    pub(super) fn movements_visible(&self, receipts: MovementReceiptList) -> Checked {
        match receipts {
            MovementReceiptList::Move { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Search { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Discard { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Destroy { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Banish { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Order { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Transform { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
            MovementReceiptList::Fusion { receipts } => {
                self.visible_object(self.env.access.get(&receipts.key()))
            }
        }
    }

    pub(super) fn install(
        &mut self,
        subjects: &InstallationSubjects,
        capture: InstallationTime,
        effect: &Effect,
        duration: &Duration,
    ) -> Checked {
        if capture != InstallationTime::InstructionStart {
            return self.reject();
        }
        let InstallationSubjects::Objects { objects, affected } = subjects else {
            return self.reject();
        };
        self.selector(objects)?;
        self.installed_duration(duration)?;
        self.isolated(|checker| {
            let scope = affected.key().0;
            if scope == checker.env.scope || !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(*affected, BindingKind::Value)?;
            checker.at("effect", |checker| match effect {
                Effect::CostModifier {
                    subjects: targets,
                    change,
                    duration: effect_duration,
                    usage: None,
                } if *targets
                    == (Selector::Bound {
                        reference: *affected,
                    })
                    && effect_duration == duration =>
                {
                    checker.selector(targets)?;
                    match change {
                        CostChange::Set {
                            amount: Expr::Literal(amount),
                        } if amount.0 < 0 => checker.reject(),
                        CostChange::Delta { amount } | CostChange::Set { amount } => {
                            checker.integer(amount)
                        }
                    }
                }
                Effect::Continuous { .. }
                | Effect::Replacement { .. }
                | Effect::Permission { .. }
                | Effect::RuleModifier { .. }
                | Effect::DeckRule { .. }
                | Effect::CostModifier { .. } => checker.reject(),
            })
        })
    }

    fn installed_duration(&mut self, duration: &Duration) -> Checked {
        if duration.anchor != (TimeAnchor::Installation {})
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
        {
            return self.reject();
        }
        match &duration.end {
            DurationEnd::CurrentTurnEnd {}
            | DurationEnd::EndResolution {}
            | DurationEnd::NoTemporalExpiry {} => Ok(()),
            DurationEnd::NextTurnEnd { player }
            | DurationEnd::NextPhaseEdge { player, .. }
            | DurationEnd::NextTurnEdge { player, .. } => self.player(player),
            DurationEnd::While { predicate } => self.boolean(predicate),
            DurationEnd::SourceZoneExit { source, .. } => self.object(source),
        }
    }
}
