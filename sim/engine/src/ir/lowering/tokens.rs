use super::{
    Action, Binding, BodyContext, Expr, LoweringError, Nat, PlayerRef, ZoneKind, ast, unsupported,
};
use crate::ir::{CardName, Posture, TokenDestination, TokenSpec, TokenTemplateId, ZoneRef};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Token creation shares the body binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn create(
        &mut self,
        value: &ast::Create,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::CreateZone::Ex = value.to;
        if value.result.is_some() && value.bind.is_some() {
            return Err(unsupported(path));
        }
        let output = Binding::new(self.card_allocator.id(path)?, self.scope);
        if let Some(name) = value.result.as_ref().or(value.bind.as_ref())
            && (self.searches.contains_key(name)
                || self.observations.contains_key(name)
                || self.targets.insert(name.clone(), output).is_some())
        {
            return Err(unsupported(path));
        }
        Ok(Action::Token {
            tokens: vec![TokenSpec {
                template: TokenTemplateId(value.token.clone()),
                name: Expr::Literal(CardName(value.token.clone())),
                count: Expr::Literal(Nat(value.n.unwrap_or(1))),
            }],
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Ex,
                }),
            },
            output,
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        })
    }
}
