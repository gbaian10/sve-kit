use super::{
    Ability, AbilityId, Allocator, BodyContext, Located, LoweringError, ScopeId, ability_source,
    ast, numeric, unsupported,
};
use crate::ir::events::timing::{opponent, phase_predicate};
use crate::ir::{
    AbilityKind, AbilityQualification, Binding, Bool, Effect, EventPattern, Expr, Nat, Phase,
    PlayerRef, RuleModifier, ZoneKind,
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
        let ast::StaticEffect::ExtraTrigger(shape) = effect else {
            return Err(unsupported(path));
        };
        let phase = match shape.phase {
            ast::TriggerPhase::Main => Phase::Main,
            ast::TriggerPhase::End => Phase::End,
        };
        let controller = match shape.who {
            ast::LeaderSide::Controller => PlayerRef::Controller {},
            ast::LeaderSide::Opponent => opponent(),
        };
        let binding = Binding::new(1, context.card_allocator.scope(path)?);
        declarations.push(Effect::RuleModifier {
            modifier: RuleModifier::TriggerMultiplicity {
                pattern: EventPattern::Phase {
                    binding,
                    predicate: phase_predicate(binding, phase, None),
                },
                controller,
                add: Nat(shape.n),
            },
            condition: condition.clone(),
            duration: numeric::duration(None, path)?,
            source: origin.clone(),
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
