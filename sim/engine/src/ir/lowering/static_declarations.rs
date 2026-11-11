use super::{
    Ability, Allocator, CardCoordinates, Located, LoweringError, ScopeId, ast, other_costs,
    prevention, source_span, static_keywords, unsupported,
};
use core::slice::from_ref;

type Lower = fn(
    &mut Allocator,
    &str,
    &ast::Ability,
    ScopeId,
    super::SourceSpan,
    &[ast::StaticEffect],
    &mut Allocator,
) -> Result<Located<Ability>, LoweringError>;

fn provider(effect: &ast::StaticEffect) -> Option<Lower> {
    if static_keywords::grant(effect).is_some() {
        return Some(static_keywords::lower_at_source);
    }
    match effect {
        ast::StaticEffect::Prevent(_)
        | ast::StaticEffect::DamageMod(_)
        | ast::StaticEffect::DamageCap(_)
        | ast::StaticEffect::If(_) => Some(prevention::lower_at_source),
        ast::StaticEffect::CostMod(_) | ast::StaticEffect::EvolveCostMod(_) => {
            Some(other_costs::lower_at_source)
        }
        ast::StaticEffect::UnlimitedEvolve(_)
        | ast::StaticEffect::ChoiceAny(_)
        | ast::StaticEffect::Reroll(_)
        | ast::StaticEffect::AlsoNamed(_)
        | ast::StaticEffect::Grant(_)
        | ast::StaticEffect::Cannot(_)
        | ast::StaticEffect::NoTrigger(_)
        | ast::StaticEffect::IgnoreWard(_) => None,
    }
}

pub(super) fn supports(head: &ast::Static) -> bool {
    !head.effects.is_empty() && head.effects.iter().all(|effect| provider(effect).is_some())
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let source = source_span(key, path, entry)?;
    let mut output: Option<Located<Ability>> = None;
    // Candidate bindings belong to the whole ability, even across provider contexts.
    let mut bindings = Allocator::default();
    for effect in &head.effects {
        let lower = provider(effect).ok_or_else(|| unsupported(path))?;
        let mut next = lower(
            allocator,
            path,
            entry,
            scope,
            source.clone(),
            from_ref(effect),
            &mut bindings,
        )?;
        if let Some(output) = &mut output {
            if next.node.active_zones != output.node.active_zones {
                return Err(unsupported(path));
            }
            output.node.declarations.append(&mut next.node.declarations);
        } else {
            output = Some(next);
        }
    }
    output.ok_or_else(|| unsupported(path))
}
