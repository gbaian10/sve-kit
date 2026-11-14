use super::{
    Ability, AbilityId, Allocator, BodyContext, Located, LoweringError, ScopeId, ability_source,
    ast, numeric, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, Bool, Effect, EffectDefinitionId, EventRewrite, Expr,
    MoveDestination, OncePerEvent, OwnerZone, PlayerRef, ProposedEventPattern, ReplacementScope,
    ZoneKind,
};
use core::mem::swap;

pub(super) fn lower_at_source(
    allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: super::SourceSpan,
    effects: &[ast::StaticEffect],
    bindings: &mut Allocator,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head
        .zone
        .as_ref()
        .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    swap(&mut context.allocator, bindings);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    let mut declarations = Vec::new();
    for effect in effects {
        let ast::StaticEffect::BanishInstead(shape) = effect else {
            return Err(unsupported(path));
        };
        if shape.from != "field" || shape.to != "cemetery" {
            return Err(unsupported(path));
        }
        let objects = shape.on.as_ref().map_or_else(
            || Ok(numeric::holder()),
            |objects| context.objects(objects, path),
        )?;
        declarations.push(Effect::Replacement {
            id: EffectDefinitionId(context.card_allocator.id(path)?),
            source: origin.clone(),
            pattern: ProposedEventPattern::Move {
                objects,
                from: ZoneKind::Field,
                to: ZoneKind::Cemetery,
            },
            condition: condition.clone(),
            rewrite: EventRewrite::RedirectMove {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Banish,
                },
                position: None,
            },
            scope: ReplacementScope::General,
            affected_player: PlayerRef::ProposedMoveController {},
            once_per_event: OncePerEvent::Enforced,
            usage: None,
            prospective_chars: None,
            duration: numeric::duration(None, path)?,
        });
    }
    swap(&mut context.allocator, bindings);
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
