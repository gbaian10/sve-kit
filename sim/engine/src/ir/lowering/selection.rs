use super::super::{Decision, DecisionId, DecisionKind, Declinable, Visibility};
use super::{BodyContext, Located, LoweringError, Node, PlayerRef, SourceSpan, ast, unsupported};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resolution choices share the ability's object binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn choose(
        &mut self,
        choice: &ast::Choose,
        path: &str,
        source: &SourceSpan,
    ) -> Result<Located<Node>, LoweringError> {
        // Exact zero needs the distinct ForcedZeroUnits instruction-start contract.
        if choice.n == Some(0) || choice.reveal == Some(true) || choice.order == Some(true) {
            return Err(unsupported(path));
        }
        let (n, upto) = if choice.may == Some(true) {
            match (choice.n, choice.upto) {
                (Some(1), None) => (None, Some(1)),
                (None, Some(_)) => (None, choice.upto),
                _ => return Err(unsupported(path)),
            }
        } else {
            (choice.n, choice.upto)
        };
        let target = ast::Target {
            bind: choice.bind.clone(),
            from: choice.from.clone(),
            distinct_by: choice.distinct_by.clone(),
            n,
            upto,
            option: None,
        };
        let (domain, cardinality) = self.object_choice(&target, path, true)?;
        let visibility = if target.from.observation.is_some() {
            Visibility::Only {
                players: super::super::PlayerSet {
                    relation: super::super::PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: super::super::Cardinality {
                        minimum: super::super::Nat(1),
                        maximum: Some(super::super::Nat(1)),
                    },
                    text_quantifier: super::super::TextQuantifier::Implicit,
                },
            }
        } else {
            Visibility::Public {}
        };
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        Ok(Located {
            source: located,
            node: Node::Decision {
                decision: Decision {
                    id: DecisionId(self.card_allocator.id(path)?),
                    captures: vec![],
                    actor: PlayerRef::Controller {},
                    domain,
                    kind: DecisionKind::ResolutionChoice {
                        may: if choice.may == Some(true) {
                            Declinable::MayDecline
                        } else {
                            Declinable::Required
                        },
                        cardinality,
                        precondition: None,
                    },
                    visibility,
                },
            },
        })
    }
}

pub(super) fn field_filter(
    kind: Option<&ast::SearchKind>,
    trait_id: Option<&str>,
    name: Option<&str>,
    contains: Option<&str>,
    candidate: super::Binding<super::super::ObjectRef>,
) -> super::Expr<super::Bool> {
    let base = super::search::object_filter(kind, trait_id, name, candidate);
    let Some(contains) = contains else {
        return base;
    };
    super::Expr::Compute(Box::new(super::Predicate::And {
        predicates: vec![
            base,
            super::Expr::Compute(Box::new(super::Predicate::NameContains {
                object: super::ObjectView::Current {
                    object: Box::new(super::ObjectOperand::Bound { binding: candidate }),
                },
                name: super::Expr::Literal(super::super::CardName(contains.to_owned())),
            })),
        ],
    }))
}
