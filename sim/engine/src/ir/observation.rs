//! Receipt spines retain each consumer's input and exclusion authority.

use super::{
    LookOutcome, MemberProjection, MovementReceipt, ObjectReceipt, ReceiptLocation,
    ReceiptReference, RevealUntilMembers, RevealUntilOutcome, Selector,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consumer {
    Order,
    ObservedMove,
    BottomShuffle,
    Reveal,
    Choice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Position {
    Input,
    Except,
}

#[derive(Debug)]
pub(crate) enum LeafKind<'selector> {
    Look(&'selector ReceiptReference<LookOutcome>),
    MovementBefore(&'selector MovementReceipt),
    RevealUntil {
        receipt: &'selector ReceiptReference<RevealUntilOutcome>,
        members: RevealUntilMembers,
        projection: MemberProjection,
    },
    Public,
}

#[derive(Debug)]
pub(crate) struct Leaf<'selector> {
    pub selector: &'selector Selector,
    pub kind: LeafKind<'selector>,
    // A projection reader never replaces Order's existing public-capture proof.
    pub public_order: bool,
}

#[derive(Debug)]
pub(crate) enum Shape<'selector> {
    Leaf(Leaf<'selector>),
    Union(&'selector [Selector]),
    Difference {
        input: &'selector Selector,
        except: &'selector Selector,
    },
}

pub(crate) fn classify(
    selector: &Selector,
    consumer: Consumer,
    position: Position,
) -> Option<Shape<'_>> {
    let (kind, public_order) = match selector {
        Selector::Union { sets }
            if matches!(consumer, Consumer::Order | Consumer::ObservedMove) =>
        {
            return Some(Shape::Union(sets));
        }
        Selector::Difference { input, except }
            if matches!(consumer, Consumer::Order | Consumer::ObservedMove)
                || (consumer == Consumer::BottomShuffle && position == Position::Input) =>
        {
            return Some(Shape::Difference { input, except });
        }
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look { receipt },
            projection,
        } if matches!(consumer, Consumer::Order | Consumer::ObservedMove)
            || (position == Position::Input
                && *projection == MemberProjection::Before
                && (matches!(consumer, Consumer::BottomShuffle | Consumer::Choice)
                    || (consumer == Consumer::Reveal
                        && matches!(receipt.location(), ReceiptLocation::Local { .. })))) =>
        {
            (LeafKind::Look(receipt), false)
        }
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Movement { receipt },
            projection: MemberProjection::Before,
        } if consumer == Consumer::Order
            || (position == Position::Except
                && (consumer == Consumer::BottomShuffle
                    || (consumer == Consumer::ObservedMove
                        && matches!(
                            receipt,
                            MovementReceipt::Move { .. } | MovementReceipt::Order { .. }
                        )))) =>
        {
            (
                LeafKind::MovementBefore(receipt),
                consumer == Consumer::Order
                    && !(position == Position::Except
                        && matches!(receipt, MovementReceipt::Order { .. })),
            )
        }
        Selector::ReceiptMembers {
            receipt:
                ObjectReceipt::RevealUntil {
                    receipt,
                    members: RevealUntilMembers::Revealed,
                },
            projection,
        } if consumer == Consumer::Order
            || (consumer == Consumer::BottomShuffle
                && position == Position::Input
                && *projection == MemberProjection::Before) =>
        {
            (
                LeafKind::RevealUntil {
                    receipt,
                    members: RevealUntilMembers::Revealed,
                    projection: *projection,
                },
                false,
            )
        }
        _ if consumer == Consumer::Order => (LeafKind::Public, true),
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
        | Selector::Difference { .. }
        | Selector::Ordered { .. }
        | Selector::Objects { .. }
        | Selector::Captured { .. }
        | Selector::Bound { .. }
        | Selector::Relation { .. } => return None,
    };
    Some(Shape::Leaf(Leaf {
        selector,
        kind,
        public_order,
    }))
}

pub(crate) fn visit<'selector>(
    selector: &'selector Selector,
    consumer: Consumer,
    visitor: &mut impl FnMut(Leaf<'selector>),
) -> bool {
    visit_at(selector, consumer, Position::Input, visitor)
}

fn visit_at<'selector>(
    selector: &'selector Selector,
    consumer: Consumer,
    position: Position,
    visitor: &mut impl FnMut(Leaf<'selector>),
) -> bool {
    match classify(selector, consumer, position) {
        Some(Shape::Leaf(leaf)) => {
            visitor(leaf);
            true
        }
        Some(Shape::Union(sets)) => sets
            .iter()
            .all(|set| visit_at(set, consumer, position, visitor)),
        Some(Shape::Difference { input, except }) => {
            visit_at(input, consumer, position, visitor)
                && visit_at(except, consumer, Position::Except, visitor)
        }
        None => false,
    }
}
