//! Unordered deck placement is closed by its immediately following shuffle.
use super::{
    Action, FollowPermission, Instruction, InstructionContext, Located, MemberProjection,
    MoveDestination, MoveReason, MovementReceipt, Node, ObjectReceipt, OwnerZone, PlayerRef,
    Position, RandomnessPolicy, ReceiptId, ReceiptLocation, Selector, ShuffleScope,
};
use core::slice::from_ref;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Return,
    Bottom,
}

pub(crate) fn observation(selector: &Selector) -> bool {
    match selector {
        Selector::ReceiptMembers {
            receipt:
                ObjectReceipt::Look { .. }
                | ObjectReceipt::RevealUntil {
                    members: super::RevealUntilMembers::Revealed,
                    ..
                },
            projection: MemberProjection::Before,
        } => true,
        Selector::Difference { input, except } => {
            observation(input)
                && matches!(
                    except.as_ref(),
                    Selector::ReceiptMembers {
                        receipt: ObjectReceipt::Movement { .. },
                        projection: MemberProjection::Before,
                    }
                )
        }
        Selector::SearchFound { .. }
        | Selector::SearchRevealed { .. }
        | Selector::CollectedMembers { .. }
        | Selector::MoveAfterAll { .. }
        | Selector::Created { .. }
        | Selector::ReceiptMembers { .. }
        | Selector::MoveAfter { .. }
        | Selector::Zone { .. }
        | Selector::Extremum { .. }
        | Selector::Filter { .. }
        | Selector::Union { .. }
        | Selector::Intersection { .. }
        | Selector::Ordered { .. }
        | Selector::Objects { .. }
        | Selector::Captured { .. }
        | Selector::Bound { .. }
        | Selector::Relation { .. } => false,
    }
}

pub(crate) fn placement(action: &Action) -> Option<Placement> {
    let Action::Move {
        objects,
        movement,
        reason: MoveReason::Put,
    } = action
    else {
        return None;
    };
    if movement.to
        != (MoveDestination::Owners {
            zone: OwnerZone::Deck,
        })
        || movement.order.is_some()
        || movement.initial_posture.is_some()
        || !movement.entry_effects.is_empty()
    {
        return None;
    }
    match movement.position {
        None => Some(Placement::Return),
        Some(Position::Bottom {}) if observation(objects) => Some(Placement::Bottom),
        _ => None,
    }
}

pub(crate) const fn consumed(action: &Action) -> Option<ReceiptId> {
    let Action::Shuffle {
        scope:
            ShuffleScope::Subset {
                cards:
                    Selector::MoveAfter {
                        receipt: MovementReceipt::Move { receipt },
                        permission: FollowPermission::SameEffectExplicitReference {},
                    },
            },
        ..
    } = action
    else {
        return None;
    };
    match receipt.location() {
        ReceiptLocation::Local { id } => Some(*id),
        ReceiptLocation::Iteration { .. } => None,
    }
}

pub(crate) const fn action(action: &Action) -> bool {
    matches!(action, Action::Shuffle {
        deck: PlayerRef::Controller {} | PlayerRef::Bound { .. }, scope,
        randomness: RandomnessPolicy::FreshRequest,
    } if matches!(scope, ShuffleScope::FullZone {})
        || matches!(action, Action::Shuffle { deck: PlayerRef::Controller {}, .. }) && consumed(action).is_some())
}

fn instruction(node: &Located<Node>) -> Option<&Instruction> {
    match &node.node {
        Node::Instruction { instruction } => Some(instruction),
        Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => None,
    }
}

fn pair(left: &Located<Node>, right: &Located<Node>) -> bool {
    let (Some(left), Some(right)) = (instruction(left), instruction(right)) else {
        return false;
    };
    if !matches!(
        left.actor,
        PlayerRef::Controller {} | PlayerRef::Bound { .. }
    ) || right.actor != left.actor
        || left.source != right.source
        || left.context != InstructionContext::Effect
        || right.context != left.context
        || left.precondition.is_some()
        || right.precondition.is_some()
        || !action(&right.action)
        || !matches!(&right.action, Action::Shuffle { deck, .. } if *deck == left.actor)
    {
        return false;
    }
    match (&right.action, placement(&left.action)) {
        (
            Action::Shuffle {
                scope: ShuffleScope::FullZone {},
                ..
            },
            Some(Placement::Return),
        ) => true,
        (_, Some(Placement::Bottom)) => consumed(&right.action) == Some(left.receipt),
        _ => false,
    }
}

pub(crate) fn whole_return(action: &Action) -> Option<&PlayerRef> {
    let Action::Move { objects, .. } = action else {
        return None;
    };
    if placement(action) != Some(Placement::Return) {
        return None;
    }
    let selectors = if let Selector::Union { sets } = objects {
        sets.as_slice()
    } else {
        from_ref(objects)
    };
    if selectors.is_empty() || selectors.len() > 2 {
        return None;
    }
    let mut player = None;
    let mut zones = Vec::new();
    for selector in selectors {
        let Selector::Zone {
            players,
            zone,
            view: super::SelectionView::Current {},
            predicate,
            ..
        } = selector
        else {
            return None;
        };
        if !matches!(zone, super::ZoneKind::Hand | super::ZoneKind::Cemetery)
            || zones.contains(zone)
            || **predicate != super::Expr::Literal(super::Bool(true))
            || players.relation != super::PlayerRelation::SelfPlayer
            || players.cardinality
                != (super::Cardinality {
                    minimum: super::Nat(1),
                    maximum: Some(super::Nat(1)),
                })
            || players.text_quantifier != super::TextQuantifier::Implicit
            || !matches!(
                players.relative_to,
                PlayerRef::Controller {} | PlayerRef::Bound { .. }
            )
            || player.is_some_and(|previous| previous != &players.relative_to)
        {
            return None;
        }
        player = Some(&players.relative_to);
        zones.push(*zone);
    }
    player
}

pub(crate) fn closed(nodes: &[Located<Node>]) -> bool {
    nodes.iter().enumerate().all(|(index, node)| {
        let Some(instruction) = instruction(node) else {
            return true;
        };
        if placement(&instruction.action).is_some() {
            nodes
                .get(index.saturating_add(1))
                .is_some_and(|next| pair(node, next))
        } else if matches!(
            instruction.action,
            Action::Shuffle {
                scope: ShuffleScope::Subset { .. },
                ..
            }
        ) {
            index
                .checked_sub(1)
                .and_then(|previous| nodes.get(previous))
                .is_some_and(|previous| pair(previous, node))
        } else {
            true
        }
    })
}
