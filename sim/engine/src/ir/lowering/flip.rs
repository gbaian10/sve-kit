//! Face-down instructions retain the authority of a selected public evolve-deck binding.

use super::{Action, BodyContext, LoweringError, ast, unsupported};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Flip lowering shares the body's selected-object environment."
)]
impl BodyContext<'_> {
    pub(super) fn flip_down(
        &mut self,
        value: &ast::ObjectAction,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let (ast::ObjectAction::Short(objects) | ast::ObjectAction::Long { r#ref: objects, .. }) =
            value;
        if !matches!(objects, ast::Objects::Bound(name) if self.flip_targets.contains(name)) {
            return Err(unsupported(path));
        }
        Ok(Action::FlipDown {
            objects: self.objects(objects, path)?,
        })
    }
}
