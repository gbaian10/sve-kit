use super::{
    Ability, Allocator, CardCoordinates, Located, LoweringError, Nat, ScopeId, ability_source, ast,
    source_span, unsupported,
};
use crate::ir::{CounterKind, ZoneKind, counter_caps};

pub(super) fn lower(
    _allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::RuleOverride(ast::RuleOverride::CounterCap(cap))] =
        head.effects.as_slice()
    else {
        return Err(unsupported(path));
    };
    if head.r#while.is_some()
        || head
            .zone
            .as_ref()
            .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
        || cap.name.is_empty()
        || i64::try_from(cap.max).is_err()
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    Ok(Located {
        node: counter_caps::ability(
            ability_source(&source, None),
            scope,
            CounterKind(cap.name.clone()),
            Nat(cap.max),
        ),
        source,
    })
}
