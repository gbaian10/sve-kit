//! Face-down instructions retain the authority of a selected public evolve-deck binding.

use super::{Action, BodyContext, LoweringError, ast, unsupported};
use crate::ir::public_evolve;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Flip lowering shares the body's selected-object environment."
)]
impl BodyContext<'_> {
    pub(super) fn flip_down(
        &mut self,
        value: &ast::FlipDown,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let objects = match value {
            ast::FlipDown::Public { r#ref: query, .. } => {
                let objects = self.counted_objects(query, path)?;
                if !public_evolve::selector(&objects) {
                    return Err(unsupported(path));
                }
                objects
            }
            ast::FlipDown::Selected(value) => {
                let (ast::ObjectAction::Short(objects)
                | ast::ObjectAction::Long { r#ref: objects, .. }) = value;
                if !matches!(objects, ast::Objects::Bound(name) if self.flip_targets.contains(name))
                {
                    return Err(unsupported(path));
                }
                self.objects(objects, path)?
            }
        };
        Ok(Action::FlipDown { objects })
    }
}
