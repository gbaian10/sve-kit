use super::{
    Ability, Allocator, BodyContext, CardCoordinates, Located, LoweringError, ScopeId,
    ability_source, ast, source_span, unsupported,
};
use crate::ir::{Bool, Expr, ZoneKind, battle_damage_basis};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::RuleOverride(ast::RuleOverride::BattleDamageBasis(rule))] =
        head.effects.as_slice()
    else {
        return Err(unsupported(path));
    };
    if head
        .zone
        .as_ref()
        .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let objects = context.objects(&rule.on, path)?;
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    Ok(Located {
        node: battle_damage_basis::ability(
            ability_source(&source, None),
            scope,
            objects,
            condition,
        ),
        source,
    })
}
