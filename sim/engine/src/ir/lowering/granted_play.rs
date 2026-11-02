use super::{
    Ability, AbilityId, Located, LoweringError, SourceSpan, ability_source, ast, numeric,
    unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, CostChange, CostKind, Effect, Expr, Int, ScopeId, ZoneKind,
};

pub(super) fn lower(
    entry: &ast::Ability,
    scope: ScopeId,
    source: SourceSpan,
    path: &str,
) -> Result<Located<Ability>, LoweringError> {
    // Exhaustive fields prevent future play clauses from being discarded in a grant.
    let ast::PlayCosts {
        define,
        cannot_if,
        cost_delta,
        cost_set,
        additional_cost,
        optional_cost,
        optional_cost_delta,
        optional_cost_set,
    } = entry.play.as_ref().ok_or_else(|| unsupported(path))?;
    let amount = cost_delta
        .as_ref()
        .and_then(serde_json::Value::as_i64)
        .filter(|amount| *amount <= 0)
        .ok_or_else(|| unsupported(path))?;
    if !define.is_empty()
        || cannot_if.is_some()
        || cost_set.is_some()
        || additional_cost.is_some()
        || optional_cost.is_some()
        || optional_cost_delta.is_some()
        || optional_cost_set.is_some()
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Ex, ZoneKind::Resolution],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations: vec![Effect::CostModifier {
                kind: CostKind::Play,
                subjects: numeric::holder(),
                change: CostChange::Delta {
                    amount: Expr::Literal(Int(amount)),
                },
                duration: numeric::duration(None, path)?,
                usage: None,
            }],
        },
    })
}
