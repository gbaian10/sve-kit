//! Candidate conditions can only read properties supplied by their authorized adapter.

use super::{Result, unsupported, values};
use crate::ir::public_numbers;
use crate::ir::{
    AbilitiesRead, AbilityPredicate, AbilityReference, Binding, Bool, BoolRead, BooleanState,
    CardKind, CardName, CharacteristicView, ClassRead, Comparison, CounterKind, EvolutionRead,
    EvolutionState, Expr, Info, Int, IntOperation, KeywordId, KindRead, NamesRead, NatRead,
    NumberRead, ObjectOperand, ObjectRef, ObjectView, PlayerId, Posture, PostureRead, Predicate,
    Stat, TraitsRead,
};

pub(super) mod candidate;
pub(super) mod subject;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Policy {
    Selection,
    PrivateSelection,
    Search,
}

#[derive(Clone, Debug)]
pub(super) enum BooleanProperty {
    Controller(PlayerId),
    ActivePlayer(PlayerId),
    SameSource,
    KnownClass,
    State(BooleanState),
    Posture(Posture),
    Evolution(EvolutionState),
    Keyword(KeywordId),
}

#[derive(Clone, Debug)]
pub(super) enum NumberProperty {
    Cost(CharacteristicView),
    Stat(Stat),
    Counter(CounterKind),
}

#[derive(Clone, Debug)]
pub(super) enum LabelsProperty {
    Class(CharacteristicView),
    Kind(CharacteristicView),
    Traits(CharacteristicView),
    Names,
}

pub(super) trait Reader {
    fn boolean(&self, property: &BooleanProperty) -> Result<bool>;
    fn number(&self, property: &NumberProperty) -> Result<Option<i64>>;
    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool>;
    fn matches_kind(&self, view: CharacteristicView, expected: CardKind) -> Result<bool> {
        self.matches_labels(&LabelsProperty::Kind(view), &|actual| {
            actual.contains(kind_name(expected))
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SourceNames {
    Holder,
    Information,
}

pub(super) trait Resolver {
    fn number(&self, expression: &Expr<Int>) -> Result<i64>;
    fn names(&self, source: SourceNames) -> Result<Vec<CardName>>;
}

#[derive(Clone, Debug)]
enum Tree<A> {
    True,
    Atom(A),
    Not(Box<Self>),
    And(Vec<Self>),
    Or(Vec<Self>),
}

#[derive(Clone, Debug)]
enum Names {
    Literal(Vec<String>),
    Source(SourceNames),
}

#[derive(Clone, Debug)]
enum Atom {
    Kind(CharacteristicView, CardKind),
    Boolean(BooleanProperty),
    Labels(LabelsProperty, Names),
    Contains(String),
    Number(NumberProperty, Comparison, Expr<Int>),
}

#[derive(Clone, Debug)]
enum Operand<T> {
    Value(T),
    Unavailable,
}

#[derive(Clone, Debug)]
enum PreparedAtom {
    Kind(CharacteristicView, CardKind),
    Boolean(BooleanProperty),
    Labels(LabelsProperty, Operand<Vec<String>>),
    Contains(String),
    Number(NumberProperty, Comparison, Operand<i64>),
}

#[derive(Clone, Debug)]
pub(super) struct ConditionPlan {
    root: Tree<Atom>,
    policy: Policy,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedCondition {
    root: Tree<PreparedAtom>,
    policy: Policy,
}

impl ConditionPlan {
    pub(super) fn compile_public_material(
        expression: &Expr<Bool>,
        binding: Binding<ObjectRef>,
    ) -> Result<Self> {
        // Only public materials admit a holder-name exclusion at this exact position.
        let exclusion = || {
            Tree::Not(Box::new(Tree::Atom(Atom::Labels(
                LabelsProperty::Names,
                Names::Source(SourceNames::Holder),
            ))))
        };
        let root = if holder_name_exclusion(expression, binding) {
            exclusion()
        } else if let Expr::Compute(operation) = expression
            && let Predicate::And { predicates } = operation.as_ref()
            && let Some((last, rest)) = predicates.split_last()
            && holder_name_exclusion(last, binding)
        {
            let remaining = match rest {
                [] => Tree::True,
                [single] => compile(single, binding, Policy::Search)?,
                rest => compile(
                    &Expr::Compute(Box::new(Predicate::And {
                        predicates: rest.to_vec(),
                    })),
                    binding,
                    Policy::Search,
                )?,
            };
            Tree::And(vec![remaining, exclusion()])
        } else {
            compile(expression, binding, Policy::Search)?
        };
        Ok(Self {
            root,
            policy: Policy::Search,
        })
    }

    pub(super) fn compile(
        expression: &Expr<Bool>,
        binding: Binding<ObjectRef>,
        policy: Policy,
    ) -> Result<Self> {
        Ok(Self {
            root: compile(expression, binding, policy)?,
            policy,
        })
    }

    pub(super) const fn conditional(&self) -> bool {
        !matches!(self.root, Tree::True)
    }

    pub(super) fn prepare(&self, resolver: Option<&dyn Resolver>) -> Result<PreparedCondition> {
        Ok(PreparedCondition {
            root: self.root.prepare(resolver)?,
            policy: self.policy,
        })
    }
}

fn holder_name_exclusion(expression: &Expr<Bool>, binding: Binding<ObjectRef>) -> bool {
    matches!(expression, Expr::Compute(not) if matches!(not.as_ref(), Predicate::Not { predicate }
        if matches!(predicate, Expr::Compute(names) if matches!(names.as_ref(),
            Predicate::NamesIntersect { left: Expr::Live(left), right: Expr::Live(right) }
                if matches!(left.as_ref(), NamesRead::Names { object } if candidate(object, binding))
                && matches!(right.as_ref(), NamesRead::Names { object: ObjectView::Current { object } }
                    if matches!(object.as_ref(), ObjectOperand::SourceHolder {}))))))
}

impl PreparedCondition {
    pub(super) fn evaluate(&self, reader: &impl Reader) -> Result<bool> {
        self.root.evaluate(reader, self.policy == Policy::Search)
    }
}

fn candidate(view: &ObjectView, binding: Binding<ObjectRef>) -> bool {
    matches!(view, ObjectView::Current { object }
        if matches!(object.as_ref(), ObjectOperand::Bound { binding: actual } if *actual == binding))
}

fn names(expression: &Expr<Vec<CardName>>, policy: Policy) -> Option<Names> {
    match expression {
        Expr::Literal(names)
            if !names.is_empty() && names.iter().all(|name| !name.0.is_empty()) =>
        {
            Some(Names::Literal(
                names.iter().map(|name| name.0.clone()).collect(),
            ))
        }
        Expr::Live(read) => match read.as_ref() {
            NamesRead::Names {
                object: ObjectView::SourceInformation {},
            } if policy != Policy::Selection => Some(Names::Source(SourceNames::Information)),
            NamesRead::Names {
                object: ObjectView::Current { object },
            } if policy == Policy::Selection
                && matches!(object.as_ref(), ObjectOperand::SourceHolder {}) =>
            {
                Some(Names::Source(SourceNames::Holder))
            }
            NamesRead::Names { .. } => None,
        },
        _ => None,
    }
}

fn number(
    read: &IntOperation,
    binding: Binding<ObjectRef>,
    policy: Policy,
) -> Option<NumberProperty> {
    // Counters are public on public-zone candidates and readable to the searching player.
    if matches!(policy, Policy::Search | Policy::Selection)
        && super::counters::candidate_read(read, binding)
        && let IntOperation::FromNat {
            value: Expr::Live(value),
        } = read
        && let NatRead::Counter { kind, .. } = value.as_ref()
    {
        return Some(NumberProperty::Counter(kind.clone()));
    }
    let IntOperation::InformationOrZero { information } = read else {
        return None;
    };
    match information {
        NumberRead::Cost {
            object,
            view: view @ (CharacteristicView::Original | CharacteristicView::Effective),
        } if candidate(object, binding) => Some(NumberProperty::Cost(*view)),
        NumberRead::Stats {
            object,
            view: CharacteristicView::Effective,
            property,
        } if candidate(object, binding)
            && (policy != Policy::Search || *property == Stat::Attack) =>
        {
            Some(NumberProperty::Stat(*property))
        }
        _ => None,
    }
}

fn compile(
    expression: &Expr<Bool>,
    binding: Binding<ObjectRef>,
    policy: Policy,
) -> Result<Tree<Atom>> {
    match expression {
        Expr::Literal(Bool(true)) => Ok(Tree::True),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates }
                if !predicates.is_empty() =>
            {
                let children = predicates
                    .iter()
                    .map(|value| compile(value, binding, policy))
                    .collect::<Result<Vec<_>>>()?;
                if policy == Policy::Search
                    && children.iter().any(|child| matches!(child, Tree::True))
                {
                    return Err(unsupported("ambiguous search condition"));
                }
                Ok(if matches!(operation.as_ref(), Predicate::And { .. }) {
                    Tree::And(children)
                } else {
                    Tree::Or(children)
                })
            }
            Predicate::Not { predicate } => {
                let child = compile(predicate, binding, policy)?;
                if policy == Policy::Search && matches!(child, Tree::True) {
                    return Err(unsupported("ambiguous search condition"));
                }
                Ok(Tree::Not(Box::new(child)))
            }
            _ => atomic(expression, binding, policy).map(Tree::Atom),
        },
        _ => atomic(expression, binding, policy).map(Tree::Atom),
    }
}

fn keyword_filter(
    abilities: &Expr<Vec<AbilityReference>>,
    predicate: &AbilityPredicate,
    binding: Binding<ObjectRef>,
) -> bool {
    matches!(abilities, Expr::Live(read)
        if matches!(read.as_ref(), AbilitiesRead::Abilities { object, view: CharacteristicView::Effective }
            if candidate(object, binding)))
        && predicate.kind.is_none()
        && predicate.except.is_empty()
        && predicate.holder.is_none()
        && predicate.origin.is_none()
        && (predicate.keyword == Some(KeywordId::Evolution)
            || matches!(
                predicate.keyword,
                Some(KeywordId::Stack | KeywordId::EarthRite | KeywordId::Lastword)
            )
            || predicate.keyword.and_then(super::grant::keyword).is_some())
}

fn boolean_atom(
    expression: &Expr<Bool>,
    binding: Binding<ObjectRef>,
    policy: Policy,
) -> Option<BooleanProperty> {
    if let Expr::Live(read) = expression
        && let BoolRead::State { object, property } = read.as_ref()
        && candidate(object, binding)
        && (policy != Policy::Search || *property == BooleanState::TokenStatus)
        && matches!(
            property,
            BooleanState::TokenStatus
                | BooleanState::FaceUp
                | BooleanState::Racing
                | BooleanState::EnteredThisTurn
                | BooleanState::Boxed
        )
    {
        return Some(BooleanProperty::State(*property));
    }
    let Expr::Compute(operation) = expression else {
        return None;
    };
    match operation.as_ref() {
        Predicate::PostureEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } if policy != Policy::Search
            && matches!(read.as_ref(), PostureRead::Posture { object } if candidate(object,binding)) =>
        {
            Some(BooleanProperty::Posture(*expected))
        }
        Predicate::EvolutionEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } if policy != Policy::Search
            && matches!(read.as_ref(), EvolutionRead::Evolution { object } if candidate(object,binding)) =>
        {
            Some(BooleanProperty::Evolution(*expected))
        }
        Predicate::HasAbility {
            abilities,
            predicate,
        } if keyword_filter(abilities, predicate, binding) => {
            predicate.keyword.map(BooleanProperty::Keyword)
        }
        _ => None,
    }
}

fn atomic(expression: &Expr<Bool>, binding: Binding<ObjectRef>, policy: Policy) -> Result<Atom> {
    if let Some(property) = boolean_atom(expression, binding, policy) {
        return Ok(Atom::Boolean(property));
    }
    let Expr::Compute(operation) = expression else {
        return Err(unsupported("candidate condition"));
    };
    let labels = match operation.as_ref() {
        Predicate::ClassEquals {
            left: Expr::Live(read),
            right: Expr::Literal(Info::Present(class)),
        } if !class.0.is_empty()
            && matches!(read.as_ref(), ClassRead::Class { object,view } if candidate(object,binding) && (*view==CharacteristicView::Effective || (policy==Policy::Search && *view==CharacteristicView::Original))) =>
        {
            let ClassRead::Class { view, .. } = read.as_ref();
            (
                LabelsProperty::Class(*view),
                Names::Literal(vec![class.0.clone()]),
            )
        }
        Predicate::KindEquals {
            left: Expr::Live(read),
            right: Expr::Literal(kind),
        } if matches!(read.as_ref(), KindRead::Kind { object,view } if candidate(object,binding) && (*view==CharacteristicView::Effective || (policy==Policy::Search && *view==CharacteristicView::Original))) =>
        {
            let KindRead::Kind { view, .. } = read.as_ref();
            return Ok(Atom::Kind(*view, *kind));
        }
        Predicate::HasTrait {
            traits: Expr::Live(read),
            trait_id,
        } if (policy == Policy::Search || !trait_id.0.is_empty())
            && matches!(read.as_ref(), TraitsRead::Traits { object,view } if candidate(object,binding) && (*view==CharacteristicView::Effective || (policy==Policy::Search && *view==CharacteristicView::Original))) =>
        {
            let TraitsRead::Traits { view, .. } = read.as_ref();
            (
                LabelsProperty::Traits(*view),
                Names::Literal(vec![trait_id.0.clone()]),
            )
        }
        Predicate::NamesIntersect {
            left: Expr::Live(read),
            right,
        } if matches!(read.as_ref(), NamesRead::Names { object } if candidate(object,binding)) => (
            LabelsProperty::Names,
            names(right, policy).ok_or_else(|| unsupported("candidate names authority"))?,
        ),
        Predicate::NameContains {
            object,
            name: Expr::Literal(name),
        } if candidate(object, binding) && !name.0.is_empty() => {
            return Ok(Atom::Contains(name.0.clone()));
        }
        Predicate::CompareInt {
            left: Expr::Compute(read),
            comparison,
            right,
        } if public_numbers::check(right)
            && (policy == Policy::Search
                || matches!(
                    comparison,
                    Comparison::Equal | Comparison::LessEqual | Comparison::GreaterEqual
                )) =>
        {
            return Ok(Atom::Number(
                number(read, binding, policy)
                    .ok_or_else(|| unsupported("candidate number authority"))?,
                *comparison,
                right.clone(),
            ));
        }
        _ => return Err(unsupported("candidate condition atom")),
    };
    Ok(Atom::Labels(labels.0, labels.1))
}

impl Tree<Atom> {
    fn prepare(&self, resolver: Option<&dyn Resolver>) -> Result<Tree<PreparedAtom>> {
        Ok(match self {
            Self::True => Tree::True,
            Self::Not(child) => Tree::Not(Box::new(child.prepare(resolver)?)),
            Self::And(children) => Tree::And(
                children
                    .iter()
                    .map(|child| child.prepare(resolver))
                    .collect::<Result<_>>()?,
            ),
            Self::Or(children) => Tree::Or(
                children
                    .iter()
                    .map(|child| child.prepare(resolver))
                    .collect::<Result<_>>()?,
            ),
            Self::Atom(atom) => Tree::Atom(match atom {
                Atom::Kind(view, kind) => PreparedAtom::Kind(*view, *kind),
                Atom::Boolean(property) => PreparedAtom::Boolean(property.clone()),
                Atom::Contains(text) => PreparedAtom::Contains(text.clone()),
                Atom::Labels(property, source) => PreparedAtom::Labels(
                    property.clone(),
                    match source {
                        Names::Literal(values) => Operand::Value(values.clone()),
                        Names::Source(source) => match resolver {
                            Some(resolver) => Operand::Value(
                                resolver
                                    .names(*source)?
                                    .into_iter()
                                    .map(|name| name.0)
                                    .collect(),
                            ),
                            None => Operand::Unavailable,
                        },
                    },
                ),
                Atom::Number(property, comparison, expression) => PreparedAtom::Number(
                    property.clone(),
                    *comparison,
                    match expression {
                        Expr::Literal(Int(value)) => Operand::Value(*value),
                        _ => match resolver {
                            Some(resolver) => Operand::Value(resolver.number(expression)?),
                            None => Operand::Unavailable,
                        },
                    },
                ),
            }),
        })
    }
}

impl Tree<PreparedAtom> {
    fn evaluate(&self, reader: &impl Reader, short_circuit: bool) -> Result<bool> {
        match self {
            Self::True => Ok(true),
            Self::Not(child) => Ok(!child.evaluate(reader, short_circuit)?),
            Self::And(children) | Self::Or(children) => {
                let all = matches!(self, Self::And(_));
                let mut result = all;
                for child in children {
                    let value = child.evaluate(reader, short_circuit)?;
                    if all {
                        result &= value;
                    } else {
                        result |= value;
                    }
                    if short_circuit && result != all {
                        break;
                    }
                }
                Ok(result)
            }
            Self::Atom(atom) => atom.evaluate(reader),
        }
    }
}

impl PreparedAtom {
    fn evaluate(&self, reader: &impl Reader) -> Result<bool> {
        match self {
            Self::Kind(view, kind) => reader.matches_kind(*view, *kind),
            Self::Boolean(property) => reader.boolean(property),
            Self::Contains(text) => {
                reader.matches_labels(&LabelsProperty::Names, &|name| name.contains(text))
            }
            Self::Labels(property, Operand::Value(expected)) => {
                reader.matches_labels(property, &|actual| {
                    expected.iter().any(|expected| {
                        if matches!(property, LabelsProperty::Kind(_)) {
                            actual.contains(expected)
                        } else {
                            actual == expected
                        }
                    })
                })
            }
            Self::Number(property, comparison, Operand::Value(expected)) => Ok(reader
                .number(property)?
                .is_some_and(|actual| values::compare(actual, *comparison, *expected))),
            Self::Labels(_, Operand::Unavailable) | Self::Number(_, _, Operand::Unavailable) => {
                Err(unsupported("unprepared candidate condition operand"))
            }
        }
    }
}

const fn kind_name(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Follower => "フォロワー",
        CardKind::Spell => "スペル",
        CardKind::Amulet => "アミュレット",
        CardKind::EvolveFollower => "フォロワー・エボルヴ",
        CardKind::Crest => "クレスト",
    }
}
