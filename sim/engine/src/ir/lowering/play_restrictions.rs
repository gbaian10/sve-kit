use super::{
    Ability, Located, LoweringError, ScopeId, ability_source, ast, source_span, unsupported,
};
use crate::ir::play_restrictions::Restriction;

pub(super) fn lower(
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    condition: &ast::PlayRestriction,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    if entry.imports.is_some() {
        return Err(unsupported(path));
    }
    let restriction = match condition {
        ast::PlayRestriction::MyTurn(true) => Restriction::OwnTurn,
        ast::PlayRestriction::InZone(zone) if zone.r#ref == "this" && zone.zone == "ex" => {
            Restriction::FromEx
        }
        ast::PlayRestriction::MyTurn(false) | ast::PlayRestriction::InZone(_) => {
            return Err(unsupported(&format!("{path}/play/cannot_if")));
        }
    };
    let source = source_span(key, path, entry)?;
    Ok(Located {
        node: restriction.ability(ability_source(&source, None), scope),
        source,
    })
}
