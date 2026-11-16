use super::{
    Ability, Allocator, CardCoordinates, Located, LoweringError, ScopeId, ability_source, ast,
    source_span, unsupported,
};
use crate::ir::{PlayerRelation, ZoneKind, draw_restrictions};

pub(super) fn lower(
    _allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::Cannot(ast::Cannot::Draw(shape))] = head.effects.as_slice() else {
        return Err(unsupported(path));
    };
    if head.r#while.is_some()
        || head
            .zone
            .as_ref()
            .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let ast::DrawRestrictionDuring {
        not_phase: ast::StartPhase::Start,
    } = shape.during;
    let relation = match shape.who {
        ast::EntryPlayer::Controller => PlayerRelation::SelfPlayer,
        ast::EntryPlayer::Opponent => PlayerRelation::Opponents,
    };
    let source = source_span(key, path, entry)?;
    Ok(Located {
        node: draw_restrictions::ability(ability_source(&source, None), scope, relation),
        source,
    })
}
