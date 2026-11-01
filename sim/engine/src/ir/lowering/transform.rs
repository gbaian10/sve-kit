use super::{Action, BodyContext, LoweringError, ast, unsupported};
use crate::ir::{TokenAllocation, TokenTemplateId};

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
        Ok(Action::Transform {
            objects: self.objects(&value.reference, path)?,
            into: TokenAllocation::Single {
                template: TokenTemplateId(value.into.clone()),
            },
            entry_effects: vec![],
        })
    }
}
