use super::{
    Ability, Allocator, BodyContext, Bool, Expr, Located, LoweringError, PlayerRef, Predicate,
    ScopeId, ZoneKind, ability_source, ast, source_span, unsupported,
};
use crate::ir::play_restrictions;

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
            vec![ZoneKind::Hand, ZoneKind::Ex],
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
                vec![ZoneKind::Hand, ZoneKind::Ex],
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
