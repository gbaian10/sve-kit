//! Whether chosen cost materials can never name the same card.
//!
//! Several material choices are frozen from one snapshot, so a card both could take would
//! reach payment twice. Choices that provably cannot meet need no shared reservation; any
//! shape this module cannot read is treated as possibly overlapping.
#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unlisted selectors and predicates stay possibly overlapping."
)]
use super::{
    CardKind, CharacteristicView, ChoiceDomain, ClassId, ClassRead, Decision, Expr, Info, KindRead,
    ObjectOperand, PlayerRef, PlayerRelation, Predicate, Selector, ZoneKind,
};

/// Kind filters match by type label inclusion: an evolved follower is also a follower, but
/// every printed type has one base (follower, spell, amulet or crest). Classes and kinds
/// compare only within the same characteristic view.
struct Pool<'selector> {
    zone: ZoneKind,
    relation: PlayerRelation,
    kinds: Vec<(CharacteristicView, CardKind)>,
    classes: Vec<(CharacteristicView, &'selector ClassId)>,
    excluded_classes: Vec<(CharacteristicView, &'selector ClassId)>,
}

/// The printed base a kind filter requires; kinds sharing a base may describe one card.
const fn base(kind: CardKind) -> u8 {
    match kind {
        CardKind::Follower | CardKind::EvolveFollower => 0,
        CardKind::Spell => 1,
        CardKind::Amulet => 2,
        CardKind::Crest => 3,
    }
}

fn pool(selector: &Selector) -> Option<Pool<'_>> {
    match selector {
        Selector::Difference { input, except }
            if matches!(except.as_ref(), Selector::Objects { objects }
                if objects.as_slice() == [ObjectOperand::SourceHolder {}]) =>
        {
            pool(input)
        }
        Selector::Zone {
            players,
            zone,
            predicate,
            ..
        } if players.relative_to == (PlayerRef::Controller {})
            && matches!(
                players.relation,
                PlayerRelation::SelfPlayer | PlayerRelation::Opponents
            ) =>
        {
            let mut found = Pool {
                zone: *zone,
                relation: players.relation,
                kinds: Vec::new(),
                classes: Vec::new(),
                excluded_classes: Vec::new(),
            };
            conjuncts(predicate, &mut found);
            Some(found)
        }
        _ => None,
    }
}

/// Reads only top-level conjuncts; anything else just narrows further and cannot add overlap.
fn conjuncts<'selector>(condition: &'selector Expr<super::Bool>, found: &mut Pool<'selector>) {
    let Expr::Compute(operation) = condition else {
        return;
    };
    match operation.as_ref() {
        Predicate::And { predicates } => {
            for branch in predicates {
                conjuncts(branch, found);
            }
        }
        Predicate::KindEquals {
            left: Expr::Live(read),
            right: Expr::Literal(kind),
        } => {
            let KindRead::Kind { view, .. } = read.as_ref();
            found.kinds.push((*view, *kind));
        }
        Predicate::ClassEquals {
            left: Expr::Live(read),
            right: Expr::Literal(Info::Present(class)),
        } => {
            let ClassRead::Class { view, .. } = read.as_ref();
            found.classes.push((*view, class));
        }
        Predicate::Not { predicate: negated } => {
            if let Expr::Compute(inner) = negated
                && let Predicate::ClassEquals {
                    left: Expr::Live(read),
                    right: Expr::Literal(Info::Present(class)),
                } = inner.as_ref()
            {
                let ClassRead::Class { view, .. } = read.as_ref();
                found.excluded_classes.push((*view, class));
            }
        }
        _ => {}
    }
}

fn pools_disjoint(left: &Pool<'_>, right: &Pool<'_>) -> bool {
    let excluded = |classes: &[(CharacteristicView, &ClassId)],
                    excluded: &[(CharacteristicView, &ClassId)]| {
        classes.iter().any(|class| excluded.contains(class))
    };
    // One characteristic view gives each card a single class.
    let different = left.classes.iter().any(|(view, class)| {
        right
            .classes
            .iter()
            .any(|(other_view, other)| view == other_view && class != other)
    });
    let kinds = left.kinds.iter().any(|(view, kind)| {
        right
            .kinds
            .iter()
            .any(|(other_view, other)| view == other_view && base(*kind) != base(*other))
    });
    left.zone != right.zone
        || left.relation != right.relation
        || kinds
        || excluded(&left.classes, &right.excluded_classes)
        || excluded(&right.classes, &left.excluded_classes)
        || different
}

fn selectors(decision: &Decision) -> Option<Vec<&Selector>> {
    let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
        return None;
    };
    Some(
        constraint
            .groups
            .iter()
            .map(|group| &group.selector)
            .collect(),
    )
}

/// Every selector of one material choice, or `None` for a shape this module cannot read.
pub(crate) fn choice_selectors(decision: &Decision) -> Option<Vec<&Selector>> {
    selectors(decision)
}

/// True when no card can satisfy two of these material sets at once; each set lists the
/// selectors one choice draws from.
pub(crate) fn disjoint_sets(sets: &[Vec<&Selector>]) -> bool {
    let Some(pools) = sets
        .iter()
        .map(|set| {
            set.iter()
                .map(|selector| pool(selector))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    pools.iter().enumerate().all(|(index, left)| {
        pools.iter().skip(index.saturating_add(1)).all(|right| {
            left.iter()
                .all(|a| right.iter().all(|b| pools_disjoint(a, b)))
        })
    })
}

/// True when no card can satisfy two of these material choices at once.
pub(crate) fn disjoint(decisions: &[&Decision]) -> bool {
    decisions
        .iter()
        .map(|decision| selectors(decision))
        .collect::<Option<Vec<_>>>()
        .is_some_and(|sets| disjoint_sets(&sets))
}
