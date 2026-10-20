use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::ReceiptLocation;
use crate::ir::{
    Binding, Bool, Cardinality, ChoiceDomain, Decision, DecisionKind, Declinable, Disjointness,
    Expr, FollowPermission, MoveDestination, MoveReason, MoveSpec, MovementReceipt,
    MovementReceiptList, Nat, ObjectRef, OwnerZone, Placement, PlayerRef, PlayerRelation, Position,
    SearchContinuation, SearchDestination, SelectionConstraint, SelectionView, Selector,
    Visibility, ZoneKind,
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

    pub(super) fn movement(
        &mut self,
        movement: &MoveSpec,
        reason: Option<MoveReason>,
        objects: &Selector,
        actor: &PlayerRef,
    ) -> Checked {
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
        if !movement.entry_effects.is_empty() {
            return self.reject();
        }
        if let Some(order) = &movement.order {
            if movement.to
                != (MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                })
                || movement.position.is_none()
            {
                return Err(self.error(WellFormedReason::InvalidMovement));
            }
            self.at("order", |checker| checker.move_order(order, objects, actor))?;
        }
        Ok(())
    }

    fn move_order(
        &mut self,
        decision: &Decision,
        objects: &Selector,
        actor: &PlayerRef,
    ) -> Checked {
        let ChoiceDomain::Order {
            objects: domain,
            binding,
        } = &decision.domain
        else {
            return self.reject();
        };
        if domain != objects || &decision.actor != actor || !decision.captures.is_empty() {
            return self.reject();
        }
        if decision.kind
            != (DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                precondition: None,
            })
        {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        self.isolated(|checker| {
            checker.decision_header(decision)?;
            checker.env.chooser = Some(actor.clone());
            checker.env.private_choice = super::visibility::private_to(&decision.visibility, actor);
            checker.selector(domain)?;
            checker.define(*binding, BindingKind::Value)
        })
    }

    pub(super) fn order(
        &mut self,
        cards: &Selector,
        destinations: &[Placement],
        chooser: &PlayerRef,
        visibility: &Visibility,
    ) -> Checked {
        if !self.env.choices {
            return self.reject();
        }
        // The common remainder-to-bottom form is one permutation, not a new subset choice.
        let [placement] = destinations else {
            return self.reject();
        };
        if placement.cards != *cards
            || placement.zone
                != (MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                })
        {
            return Err(self.error(WellFormedReason::InvalidMovement));
        }
        self.player(chooser)?;
        self.visibility(visibility)?;
        self.isolated(|checker| {
            checker.env.chooser = Some(chooser.clone());
            checker.env.private_choice = super::visibility::private_to(visibility, chooser);
            checker.at("cards", |checker| checker.selector(cards))
        })
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
        if !self.env.choices {
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
        if let Some(groups) = groups {
            self.at("groups", |checker| {
                checker.search_groups(groups, player, deck, cardinality)
            })?;
        }
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
            checker.env.access.insert(
                continuation.found.key(),
                super::visibility::only(player.clone()),
            );
            checker
                .env
                .access
                .insert(continuation.revealed.key(), Visibility::Public {});
            let objects = Selector::SearchFound {
                found: continuation.found,
            };
            checker.at("destination", |checker| match destination {
                None => checker.reject(),
                Some(SearchDestination::Single { movement }) => {
                    checker.search_movement(movement, &objects, player)
                }
                Some(SearchDestination::OptionalSelfReplacement {
                    predicate,
                    movement,
                }) => {
                    checker.boolean(predicate)?;
                    checker.search_movement(movement, &objects, player)
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

    fn search_movement(
        &mut self,
        movement: &MoveSpec,
        objects: &Selector,
        player: &PlayerRef,
    ) -> Checked {
        // Search ends with a shuffle; placement order needs a separate sequencing contract.
        if movement.order.is_some() || movement.position.is_some() {
            return self.reject();
        }
        self.movement(movement, None, objects, player)
    }

    fn search_groups(
        &mut self,
        groups: &SelectionConstraint,
        player: &PlayerRef,
        deck: &PlayerRef,
        cardinality: Cardinality,
    ) -> Checked {
        if groups.cardinality != cardinality
            || groups.disjoint != Disjointness::Disjoint
            || groups.groups.is_empty()
        {
            return self.reject();
        }
        self.isolated(|checker| {
            checker.env.chooser = Some(player.clone());
            checker.env.private_choice = true;
            for (index, group) in groups.groups.iter().enumerate() {
                checker.at(&index.to_string(), |checker| {
                    checker.cardinality(group.cardinality)?;
                    let Selector::Zone {
                        players,
                        zone: ZoneKind::Deck,
                        view: SelectionView::Current {},
                        binding,
                        predicate,
                    } = &group.selector
                    else {
                        return checker.reject();
                    };
                    if players.relative_to != *deck
                        || players.relation != PlayerRelation::SelfPlayer
                    {
                        return checker.reject();
                    }
                    checker.players(players)?;
                    // Only the root candidate domain is authorized; nested hidden reads are not.
                    checker.candidate(*binding, predicate)
                })?;
            }
            checker.aggregate_bounds(&groups.aggregate_bounds)
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

    pub(super) fn movement_before_visible(&self, receipt: &MovementReceipt) -> Checked {
        movement_dispatch!(receipt, MovementReceipt, receipt => {
            match receipt.location() {
                ReceiptLocation::Local { id } => self.visible_object(self.env.movement_before_access.get(id)),
                ReceiptLocation::Iteration { .. } => self.reject(),
            }
        }; trigger => self.reject())
    }

    pub(super) fn movements_visible(&self, receipts: MovementReceiptList) -> Checked {
        movement_dispatch!(receipts, MovementReceiptList, receipts => self.visible_object(self.env.access.get(&receipts.key())))
    }
}
