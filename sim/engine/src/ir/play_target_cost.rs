//! A quote may borrow only the unique spell's actual public formal selection.
use super::{
    AbilityKind, AbilityOrigin, Bool, BoolRead, CardProgram, Cardinality, ChoiceDomain, Decision,
    DecisionKind, ExecutionRef, Expr, InputMode, Nat, ObjectOperand, PlaySlot, PlayTargetMatch,
    PlayerRef, Predicate, SelectionSnapshot, SelectionView, Selector, SourceSpan, Visibility,
    ZoneKind,
};

pub(crate) fn parameter<'card>(
    card: &'card CardProgram,
    source: &SourceSpan,
    read: &BoolRead,
) -> Option<&'card Decision> {
    let BoolRead::CardPlayTarget {
        execution: ExecutionRef::Current {},
        decision,
        selection,
        predicate,
    } = read
    else {
        return None;
    };
    match predicate {
        PlayTargetMatch::NameEquals { name } if name.0.is_empty() => return None,
        PlayTargetMatch::NameContains { fragment } if fragment.0.is_empty() => return None,
        PlayTargetMatch::Present {}
        | PlayTargetMatch::NameEquals { .. }
        | PlayTargetMatch::NameContains { .. } => {}
    }
    let mut spells = card.abilities.iter().filter(|ability| {
        ability.node.kind == AbilityKind::Spell && ability.source.face == source.face
    });
    let spell = spells.next()?;
    if spells.next().is_some()
        || spell.source.card_id != source.card_id
        || spell.source.card_no != source.card_no
        || spell.node.source.holder != (ObjectOperand::SourceHolder {})
        || spell.node.source.controller != (PlayerRef::Controller {})
        || !matches!(&spell.node.source.origin, AbilityOrigin::Printed { face, .. } if face.0 == source.face)
    {
        return None;
    }
    let parameter = spell
        .node
        .play
        .as_ref()?
        .parameters
        .iter()
        .find(|parameter| parameter.id == *decision)?;
    if parameter.actor != (PlayerRef::Controller {})
        || parameter.visibility != (Visibility::Public {})
        || !matches!(
            parameter.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::Targets,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            }
        )
    {
        return None;
    }
    let ChoiceDomain::Objects {
        binding,
        constraint,
        target_roles: None,
    } = &parameter.domain
    else {
        return None;
    };
    (*binding == *selection
        && constraint.cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
        && !constraint.groups.is_empty()
        && constraint
            .groups
            .iter()
            .all(|group| public_board(&group.selector)))
    .then_some(parameter)
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only current public field and EX selectors carry this quote authority."
)]
fn public_board(selector: &Selector) -> bool {
    match selector {
        Selector::Zone {
            zone: ZoneKind::Field | ZoneKind::Ex,
            view: SelectionView::Current {},
            ..
        } => true,
        Selector::Filter { input, .. } | Selector::Difference { input, .. } => public_board(input),
        Selector::Union { sets } | Selector::Intersection { sets } => {
            !sets.is_empty() && sets.iter().all(public_board)
        }
        _ => false,
    }
}

/// Other expressions keep their normal WF checks; only a proven cross-head read is masked.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unrelated expressions remain subject to the ordinary checker."
)]
pub(crate) fn mask(condition: &mut Expr<Bool>, card: &CardProgram, source: &SourceSpan) -> bool {
    match condition {
        Expr::Live(read) if matches!(read.as_ref(), BoolRead::CardPlayTarget { .. }) => {
            if parameter(card, source, read).is_none() {
                return false;
            }
            *condition = Expr::Literal(Bool(false));
            true
        }
        Expr::Compute(operation) => match operation.as_mut() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                predicates.iter_mut().all(|value| mask(value, card, source))
            }
            Predicate::Not { predicate } => mask(predicate, card, source),
            _ => true,
        },
        _ => true,
    }
}

/// Identify exact formal decisions before an affordability probe fills their real answers.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Other cost expressions have no formal target dependency."
)]
pub(crate) fn reads(condition: &Expr<Bool>, decision: Option<super::DecisionId>) -> bool {
    match condition {
        Expr::Live(read) => {
            matches!(read.as_ref(), BoolRead::CardPlayTarget { decision: actual, .. } if decision.is_none_or(|expected| expected == *actual))
        }
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                predicates.iter().any(|value| reads(value, decision))
            }
            Predicate::Not { predicate } => reads(predicate, decision),
            _ => false,
        },
        _ => false,
    }
}
