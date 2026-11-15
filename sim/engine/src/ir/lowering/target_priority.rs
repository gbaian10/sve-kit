use super::{
    Ability, AbilityId, Allocator, Located, LoweringError, ScopeId, ability_source, ast,
    unsupported,
};
use crate::ir::{AbilityKind, AbilityQualification, ZoneKind, target_priority};

pub(super) fn lower_at_source(
    _allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: super::SourceSpan,
    effects: &[ast::StaticEffect],
    _bindings: &mut Allocator,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head.r#while.is_some()
        || head
            .zone
            .as_ref()
            .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    let mut declarations = Vec::new();
    for effect in effects {
        let ast::StaticEffect::MustTarget(shape) = effect else {
            return Err(unsupported(path));
        };
        if !matches!(shape.on, ast::Objects::This(_))
            || !matches!(shape.by, ast::LeaderSide::Opponent)
        {
            return Err(unsupported(path));
        }
        declarations.push(target_priority::requirement());
    }
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}
