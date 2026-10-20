use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::{
    Binding, Bool, Cardinality, CostChange, Duration, DurationEnd, Effect, Expr, FollowPermission,
    InstallationSubjects, InstallationTime, MoveDestination, MoveReason, MoveSpec, MovementReceipt,
    MovementReceiptList, ObjectRef, OwnerZone, PlayerRef, Position, SearchContinuation,
    SearchDestination, SelectionConstraint, Selector, TimeAnchor, ZoneKind,
};

// Keep receipt outcome types intact while sharing the exhaustive movement family.
macro_rules! movement_dispatch {
    ($value:expr, $family:ident, $member:ident => $check:expr $(; trigger => $trigger:expr)?) => {
        match $value {
            $family::Move { $member } => $check,
            $family::Search { $member } => $check,
            $family::Discard { $member } => $check,
            $family::Destroy { $member } => $check,
            $family::Banish { $member } => $check,
            $family::Order { $member } => $check,
            $family::Transform { $member } => $check,
            $family::Fusion { $member } => $check,
            $($family::TriggerEvent { .. } => $trigger,)?
        }
    };
}

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

    pub(super) fn positioned_destination(
        &mut self,
        to: &MoveDestination,
        position: Option<Position>,
    ) -> Checked {
        self.destination(to)?;
        if position.is_some()
            && *to
                != (MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                })
        {
            return Err(self.error(WellFormedReason::InvalidMovement));
        }
        Ok(())
    }

    pub(super) fn movement(&mut self, movement: &MoveSpec, reason: Option<MoveReason>) -> Checked {
        self.at("to", |checker| {
            checker.positioned_destination(&movement.to, movement.position)
        })?;
        if movement.initial_posture.is_some()
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
        movement_dispatch!(receipt, MovementReceipt, receipt => self.receipt(receipt); trigger => self.reject())
    }

    pub(super) fn movement_receipts(&self, receipts: MovementReceiptList) -> Checked {
        movement_dispatch!(receipts, MovementReceiptList, receipts => self.reference(receipts, BindingKind::Value))
    }

    pub(super) fn movement_visible(&self, receipt: &MovementReceipt) -> Checked {
        movement_dispatch!(receipt, MovementReceipt, receipt => self.visible_receipt(receipt); trigger => self.reject())
    }

    pub(super) fn movements_visible(&self, receipts: MovementReceiptList) -> Checked {
        movement_dispatch!(receipts, MovementReceiptList, receipts => self.visible_object(self.env.access.get(&receipts.key())))
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
