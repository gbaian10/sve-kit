use super::{
    Action, BodyContext, Cardinality, LoweringError, Nat, ObjectOperand, PlayerRef, Selector, ast,
    unsupported,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Steal lowering shares formal target bindings."
)]
impl BodyContext<'_> {
    pub(super) fn remember_transfer_target(
        &mut self,
        target: &ast::Target,
        bounds: Cardinality,
        dynamic: bool,
        resolution: bool,
    ) {
        if !resolution
            && !dynamic
            && bounds.minimum == Nat(1)
            && bounds.maximum == Some(Nat(1))
            && matches!(target.from.who, Some(ast::Side::Opponent))
            && matches!(
                target.from.zone,
                Some(ast::TargetZone::Single(ast::TargetSingleZone::Field))
            )
            && matches!(target.from.kind, Some(ast::SearchKind::Follower))
        {
            self.transfer_targets.insert(target.bind.clone());
        }
    }

    pub(super) fn steal(&self, value: &ast::Steal, path: &str) -> Result<Action, LoweringError> {
        if !self.transfer_targets.contains(&value.r#ref) {
            return Err(unsupported(path));
        }
        let selection = *self
            .targets
            .get(&value.r#ref)
            .ok_or_else(|| unsupported(path))?;
        Ok(Action::Transfer {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::Selected { selection }],
            },
            destination_controller: PlayerRef::Controller {},
        })
    }
}
