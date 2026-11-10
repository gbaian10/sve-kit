use super::{
    Ability, Allocator, CardCoordinates, Located, LoweringError, ScopeId, ability_source, ast,
    source_span, unsupported,
};
use crate::ir::{ZoneKind, choice_rules};

pub(super) fn lower(
    _allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::ChoiceAny(player)] = head.effects.as_slice() else {
        return Err(unsupported(path));
    };
    let ast::SelfPlayer::Controller = player.who;
    if head.r#while.is_some()
        || head.zone.as_ref().and_then(ast::ActivationZones::single) != Some(ZoneKind::Ex)
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    Ok(Located {
        node: choice_rules::ability(ability_source(&source, None), scope),
        source,
    })
}
