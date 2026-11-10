use super::{
    Ability, Allocator, BodyContext, Bool, Expr, Located, LoweringError, PlayerRef, Predicate,
    ScopeId, ZoneKind, ability_source, ast, source_span, unsupported,
};
use crate::ir::play_restrictions;
use crate::ir::{Action, Binding};

pub(super) fn cemetery(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    permission: &ast::ConditionalCemetery,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    if entry.imports.is_some() {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let empty = ast::Body::default();
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = context.qualification(
        &permission.condition,
        &format!("{path}/play/playable_from/if"),
    )?;
    Ok(Located {
        node: play_restrictions::cemetery_ability(ability_source(&source, None), scope, condition),
        source,
    })
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    condition: &ast::PlayRestriction,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    if entry.imports.is_some() {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let (zones, predicate) = match condition {
        ast::PlayRestriction::Named(ast::NamedPlayRestriction::MyTurn(true)) => (
            vec![
                ZoneKind::Hand,
                ZoneKind::Ex,
                ZoneKind::Cemetery,
                ZoneKind::Banish,
            ],
            Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Controller {}),
                right: Expr::Literal(PlayerRef::Active {}),
            })),
        ),
        ast::PlayRestriction::Named(ast::NamedPlayRestriction::InZone(zone))
            if zone.r#ref == "this" && zone.zone == "ex" =>
        {
            (vec![ZoneKind::Ex], Expr::Literal(Bool(true)))
        }
        ast::PlayRestriction::Condition(condition) => {
            let empty = ast::Body::default();
            let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
            (
                vec![
                    ZoneKind::Hand,
                    ZoneKind::Ex,
                    ZoneKind::Cemetery,
                    ZoneKind::Banish,
                ],
                context.qualification(condition, &format!("{path}/play/cannot_if"))?,
            )
        }
        ast::PlayRestriction::Named(_) => {
            return Err(unsupported(&format!("{path}/play/cannot_if")));
        }
    };
    Ok(Located {
        node: play_restrictions::ability(ability_source(&source, None), scope, zones, predicate),
        source,
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player play permissions share the body allocator."
)]
impl BodyContext<'_> {
    pub(super) fn player_play_permission(
        &mut self,
        value: &ast::PlayerEffect,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::PlayPermission(shape) = &value.effect else {
            return Err(unsupported(path));
        };
        if !matches!(value.who, ast::Side::Controller)
            || !matches!(value.until, Some(ast::Until::Turn))
            || !matches!(shape.from.as_slice(), [ast::BanishPermissionZone::Banish])
        {
            return Err(unsupported(path));
        }
        let ast::SelfPlayer::Controller = shape.on.who;
        let ast::BanishPermissionZone::Banish = shape.on.zone;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let candidate = Binding::new(1, self.card_allocator.scope(path)?);
        Ok(play_restrictions::banish_play_installation(
            affected, candidate,
        ))
    }
}
