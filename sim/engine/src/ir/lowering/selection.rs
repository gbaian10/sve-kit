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
        if choice.n == Some(0) {
            return Err(unsupported(path));
        }
        let target = ast::Target {
            bind: choice.bind.clone(),
            from: choice.from.clone(),
            n: choice.n,
            upto: choice.upto,
            option: None,
        };
        let (domain, cardinality) = self.object_choice(&target, path)?;
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
                        may: Declinable::Required,
                        cardinality,
                        precondition: None,
                    },
                    visibility: Visibility::Public {},
                },
            },
        })
    }
}
