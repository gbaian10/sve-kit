//! Event and Fusion adapters expose captured information without borrowing live object authority.

use super::super::source::PublicCharacteristics;
use super::super::{Result, unsupported};
use super::{
    Atom, BooleanProperty, ConditionPlan, LabelsProperty, Names, NumberProperty, Policy, Reader,
    Tree,
};
use crate::ir::events::discard::{DiscardCard, DiscardFilter};
use crate::ir::events::played::PlayedFilter;
use crate::ir::events::subject::{SubjectFilter, SubjectInformation};
use crate::ir::{
    BooleanState, CardKind, CharacteristicView, Characteristics, Comparison, Expr, Int, ObjectRef,
    PlayerId, TraitId,
};

#[derive(Clone, Copy)]
enum Context {
    Unavailable,
    Event {
        same: bool,
        controller: PlayerId,
        active: PlayerId,
        token: bool,
    },
    Hand {
        same: bool,
        controller: PlayerId,
        active: PlayerId,
    },
    Controller(PlayerId),
}

enum Information<'information> {
    Event(&'information SubjectInformation<'information>),
    Source(&'information PublicCharacteristics),
    Hand(&'information Characteristics),
    Traits(&'information [TraitId]),
    None,
}

struct Captured<'information> {
    information: Information<'information>,
    context: Context,
}

impl Context {
    const fn controller(self) -> Option<PlayerId> {
        match self {
            Self::Event { controller, .. }
            | Self::Hand { controller, .. }
            | Self::Controller(controller) => Some(controller),
            Self::Unavailable => None,
        }
    }
}

impl Reader for Captured<'_> {
    fn boolean(&self, property: &BooleanProperty) -> Result<bool> {
        match (property, self.context) {
            (BooleanProperty::KnownClass, _) => match &self.information {
                Information::Event(information) => Ok(information.class.is_some()),
                _ => Err(unsupported("captured class availability")),
            },
            (BooleanProperty::Controller(expected), context) => context
                .controller()
                .map(|controller| controller == *expected)
                .ok_or_else(|| unsupported("captured controller information")),
            (
                BooleanProperty::ActivePlayer(expected),
                Context::Event { active, .. } | Context::Hand { active, .. },
            ) => Ok(active == *expected),
            (
                BooleanProperty::SameSource,
                Context::Event { same, .. } | Context::Hand { same, .. },
            ) => Ok(same),
            (BooleanProperty::State(BooleanState::TokenStatus), Context::Event { token, .. }) => {
                Ok(token)
            }
            (BooleanProperty::Keyword(keyword), _) => match &self.information {
                Information::Event(information) => Ok(information.keywords.contains(keyword)),
                _ => Err(unsupported("captured keyword information")),
            },
            _ => Err(unsupported("captured event role or state")),
        }
    }

    fn number(&self, property: &NumberProperty) -> Result<Option<i64>> {
        match (property, &self.information) {
            (
                NumberProperty::Cost(CharacteristicView::Original),
                Information::Event(information),
            ) => Ok(information.original_cost),
            _ => Err(unsupported("captured number information")),
        }
    }

    fn matches_kind(&self, view: CharacteristicView, expected: CardKind) -> Result<bool> {
        if view != CharacteristicView::Effective {
            return Err(unsupported("captured kind view"));
        }
        let actual = match &self.information {
            Information::Event(information) => information.kind,
            Information::Source(information) => information.kind,
            Information::Hand(information) => Some(information.kind),
            Information::Traits(_) | Information::None => {
                return Err(unsupported("captured kind information"));
            }
        };
        Ok(actual == Some(expected))
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        match (property, &self.information) {
            (
                LabelsProperty::Class(CharacteristicView::Effective),
                Information::Event(information),
            ) => Ok(information.class.is_some_and(|class| matches(&class.0))),
            (
                LabelsProperty::Class(CharacteristicView::Effective),
                Information::Hand(information),
            ) => Ok(information
                .class
                .as_ref()
                .is_some_and(|class| matches(&class.0))),
            (LabelsProperty::Names, Information::Event(information)) => {
                Ok(information.names.iter().any(|name| matches(&name.0)))
            }
            (LabelsProperty::Traits(CharacteristicView::Effective), information) => {
                let traits = match information {
                    Information::Event(information) => information.traits,
                    Information::Source(information) => &information.traits,
                    Information::Traits(traits) => traits,
                    Information::Hand(_) | Information::None => {
                        return Err(unsupported("captured trait information"));
                    }
                };
                Ok(traits.iter().any(|id| matches(&id.0)))
            }
            _ => Err(unsupported("captured label information")),
        }
    }
}

fn boolean(property: BooleanProperty, positive: bool) -> Tree<Atom> {
    let atom = Tree::Atom(Atom::Boolean(property));
    if positive {
        atom
    } else {
        Tree::Not(Box::new(atom))
    }
}

const fn labels(property: LabelsProperty, values: Vec<String>) -> Tree<Atom> {
    Tree::Atom(Atom::Labels(property, Names::Literal(values)))
}

const fn original_cost(comparison: Comparison, value: i64) -> Tree<Atom> {
    Tree::Atom(Atom::Number(
        NumberProperty::Cost(CharacteristicView::Original),
        comparison,
        Expr::Literal(Int(value)),
    ))
}

fn subject(filter: &SubjectFilter, observer: PlayerId) -> Tree<Atom> {
    let mut conditions = Vec::new();
    if let Some((comparison, cost)) = filter.original_cost {
        conditions.push(original_cost(comparison, cost));
    }
    if let Some(keyword) = filter.keyword {
        conditions.push(boolean(BooleanProperty::Keyword(keyword), true));
    }
    if let Some(token) = filter.token {
        conditions.push(boolean(
            BooleanProperty::State(BooleanState::TokenStatus),
            token,
        ));
    }
    if let Some(same) = filter.itself {
        conditions.push(boolean(BooleanProperty::SameSource, same));
    }
    if let Some(own) = filter.own {
        conditions.push(boolean(BooleanProperty::Controller(observer), own));
    }
    if let Some(kind) = filter.kind {
        conditions.push(Tree::Atom(Atom::Kind(CharacteristicView::Effective, kind)));
    }
    if let Some(id) = &filter.trait_id {
        conditions.push(labels(
            LabelsProperty::Traits(CharacteristicView::Effective),
            vec![id.0.clone()],
        ));
    }
    if !filter.traits_any.is_empty() {
        conditions.push(labels(
            LabelsProperty::Traits(CharacteristicView::Effective),
            filter.traits_any.iter().map(|id| id.0.clone()).collect(),
        ));
    }
    if let Some(name) = &filter.name {
        conditions.push(labels(LabelsProperty::Names, vec![name.0.clone()]));
    }
    if let Some(fragment) = &filter.name_contains {
        conditions.push(Tree::Atom(Atom::Contains(fragment.0.clone())));
    }
    if !filter.name_contains_any.is_empty() {
        conditions.push(Tree::Or(
            filter
                .name_contains_any
                .iter()
                .map(|fragment| Tree::Atom(Atom::Contains(fragment.0.clone())))
                .collect(),
        ));
    }
    if let Some(class) = &filter.class {
        conditions.push(labels(
            LabelsProperty::Class(CharacteristicView::Effective),
            vec![class.0.clone()],
        ));
    }
    if let Some(class) = &filter.not_class {
        conditions.push(boolean(BooleanProperty::KnownClass, true));
        conditions.push(Tree::Not(Box::new(labels(
            LabelsProperty::Class(CharacteristicView::Effective),
            vec![class.0.clone()],
        ))));
    }
    if filter.own_turn {
        conditions.push(boolean(BooleanProperty::ActivePlayer(observer), true));
    }
    Tree::And(conditions)
}

fn evaluate(root: Tree<Atom>, reader: &Captured<'_>) -> Result<bool> {
    // Event predicates keep their former short circuit; the adapter supplies all read authority.
    ConditionPlan {
        root,
        policy: Policy::Search,
    }
    .prepare(None)?
    .evaluate(reader)
}

pub(in crate::game::internal) fn matches(
    filter: &SubjectFilter,
    information: &SubjectInformation<'_>,
    same: bool,
    observer: PlayerId,
    controller: PlayerId,
    active: PlayerId,
    token: bool,
) -> Result<bool> {
    // The observer's names are not captured; entry observers resolve the exclusion first.
    if filter.not_same_name {
        return Err(unsupported("captured observer names"));
    }
    evaluate(
        subject(filter, observer),
        &Captured {
            information: Information::Event(information),
            context: Context::Event {
                same,
                controller,
                active,
                token,
            },
        },
    )
}

pub(in crate::game::internal) fn played(
    filter: &PlayedFilter,
    information: &SubjectInformation<'_>,
    same: bool,
    observer: PlayerId,
    controller: PlayerId,
    active: PlayerId,
    token: bool,
) -> Result<bool> {
    let mut subjects = vec![subject(&filter.subject, observer)];
    if let Some(alternate) = &filter.alternate {
        subjects.push(subject(alternate, observer));
    }
    let mut conditions = vec![Tree::Or(subjects)];
    if let Some((comparison, value)) = filter.original {
        conditions.push(
            if matches!(
                comparison,
                Comparison::Equal | Comparison::LessEqual | Comparison::GreaterEqual
            ) {
                original_cost(comparison, value)
            } else {
                Tree::Not(Box::new(Tree::True))
            },
        );
    }
    if !filter.traits_any.is_empty() {
        conditions.push(labels(
            LabelsProperty::Traits(CharacteristicView::Effective),
            filter.traits_any.iter().map(|id| id.0.clone()).collect(),
        ));
    }
    evaluate(
        Tree::And(conditions),
        &Captured {
            information: Information::Event(information),
            context: Context::Event {
                same,
                controller,
                active,
                token,
            },
        },
    )
}

pub(in crate::game::internal) fn controlled_by(
    controller: PlayerId,
    expected: PlayerId,
) -> Result<bool> {
    evaluate(
        boolean(BooleanProperty::Controller(expected), true),
        &Captured {
            information: Information::None,
            context: Context::Controller(controller),
        },
    )
}

pub(in crate::game::internal) fn source(
    information: &PublicCharacteristics,
    kind: CardKind,
    trait_id: &TraitId,
) -> Result<bool> {
    // The source was captured before payment; no other source properties are authorized here.
    let root = Tree::And(vec![
        Tree::Atom(Atom::Kind(CharacteristicView::Effective, kind)),
        labels(
            LabelsProperty::Traits(CharacteristicView::Effective),
            vec![trait_id.0.clone()],
        ),
    ]);
    evaluate(
        root,
        &Captured {
            information: Information::Source(information),
            context: Context::Unavailable,
        },
    )
}

pub(in crate::game::internal) fn traits(
    information: &[TraitId],
    trait_id: &TraitId,
) -> Result<bool> {
    evaluate(
        labels(
            LabelsProperty::Traits(CharacteristicView::Effective),
            vec![trait_id.0.clone()],
        ),
        &Captured {
            information: Information::Traits(information),
            context: Context::Unavailable,
        },
    )
}

pub(in crate::game::internal) fn discard_roles(
    filter: &DiscardFilter,
    player: PlayerId,
    card: ObjectRef,
    source: ObjectRef,
    observer: PlayerId,
    active: PlayerId,
) -> Result<bool> {
    let mut conditions = Vec::new();
    if let Some(own) = filter.own {
        conditions.push(boolean(BooleanProperty::Controller(observer), own));
    }
    if filter.itself {
        conditions.push(boolean(BooleanProperty::SameSource, true));
    }
    if filter.own_turn {
        conditions.push(boolean(BooleanProperty::ActivePlayer(observer), true));
    }
    evaluate(
        Tree::And(conditions),
        &Captured {
            information: Information::None,
            context: Context::Hand {
                same: card == source,
                controller: player,
                active,
            },
        },
    )
}

pub(in crate::game::internal) fn discard_card(
    filter: &DiscardCard,
    information: &Characteristics,
) -> Result<bool> {
    let mut conditions = Vec::new();
    if let Some(kind) = filter.kind {
        conditions.push(Tree::Atom(Atom::Kind(CharacteristicView::Effective, kind)));
    }
    if let Some(class) = &filter.class {
        conditions.push(labels(
            LabelsProperty::Class(CharacteristicView::Effective),
            vec![class.0.clone()],
        ));
    }
    evaluate(
        Tree::And(conditions),
        &Captured {
            information: Information::Hand(information),
            context: Context::Unavailable,
        },
    )
}

#[cfg(test)]
mod tests;
