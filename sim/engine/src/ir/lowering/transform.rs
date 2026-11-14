use super::{Action, Binding, BodyContext, Expr, LoweringError, Nat, PlayerRef, ast, unsupported};
use crate::ir::{DecisionId, TokenAllocation, TokenTemplateId};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Transformation shares the body binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn transform(
        &mut self,
        value: &ast::Transform,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::Objects::Bound(name) = &value.reference else {
            return Err(unsupported(path));
        };
        if !self.ex_targets.contains(name) {
            return Err(unsupported(path));
        }
        let into = match &value.into {
            ast::TransformInto::Single(template) => TokenAllocation::Single {
                template: TokenTemplateId(template.clone()),
            },
            ast::TransformInto::Allocated { allocate } => {
                let mut names = allocate.among.clone();
                names.sort();
                names.dedup();
                if allocate.among.len() < 2
                    || names.len() != allocate.among.len()
                    || names.iter().any(|template| template.trim().is_empty())
                {
                    return Err(unsupported(path));
                }
                TokenAllocation::EachExactlyOnce {
                    templates: allocate
                        .among
                        .iter()
                        .cloned()
                        .map(TokenTemplateId)
                        .collect(),
                    total: Expr::Literal(Nat(
                        u64::try_from(allocate.among.len()).map_err(|_error| unsupported(path))?
                    )),
                    counts: Binding::new(self.allocator.id(path)?, self.scope),
                    actor: PlayerRef::Controller {},
                    decision: DecisionId(self.card_allocator.id(path)?),
                }
            }
        };
        Ok(Action::Transform {
            objects: self.objects(&value.reference, path)?,
            into,
            entry_effects: vec![],
        })
    }
}
