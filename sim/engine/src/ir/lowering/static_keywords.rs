use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::ObjectRelation;
use crate::ir::{
    AbilityKind, AbilityQualification, CharacteristicPatch, ContinuousLayer, Effect,
    EffectDefinitionId, EffectTarget, ObjectOperand, ScopeId, TimestampBasis, ZoneKind,
};
use crate::ir::{ObjectView, Selector};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let source = source_span(key, path, entry)?;
    lower_at_source(allocator, path, entry, scope, source)
}

pub(super) fn lower_at_source(
    allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: super::SourceSpan,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let zone = head
        .zone
        .as_ref()
        .map_or(Some(ZoneKind::Field), ast::ActivationZones::single)
        .ok_or_else(|| unsupported(path))?;
    if !matches!(zone, ZoneKind::Field | ZoneKind::Equipment)
        || (zone == ZoneKind::Equipment && head.r#while.is_some())
    {
        return Err(unsupported(path));
    }
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?;
    let mut declarations = Vec::new();
    for effect in &head.effects {
        let ast::StaticEffect::Grant(grant) = effect else {
            return Err(unsupported(path));
        };
        if grant.keywords.is_empty() && grant.abilities.is_empty() {
            return Err(unsupported(path));
        }
        let abilities =
            context.granted_abilities(&grant.keywords, &grant.abilities, &source, path)?;
        let selector = if zone == ZoneKind::Equipment {
            if !matches!(&grant.on, Some(ast::Objects::Bound(name)) if name == "host") {
                return Err(unsupported(path));
            }
            Selector::Relation {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                relation: ObjectRelation::Equipment,
            }
        } else {
            grant
                .on
                .as_ref()
                .map_or_else(|| Ok(numeric::holder()), |on| context.objects(on, path))?
        };
        declarations.push(Effect::Continuous {
            id: EffectDefinitionId(context.card_allocator.id(path)?),
            source: origin.clone(),
            target: EffectTarget::Dynamic { selector },
            condition: condition.clone(),
            patch: CharacteristicPatch::GrantAbility { abilities },
            layer: ContinuousLayer::Ability,
            timestamp: TimestampBasis::ZoneEntry {
                object: ObjectOperand::SourceHolder {},
            },
            active_zones: vec![zone],
            duration: numeric::duration(None, path)?,
        });
    }
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![zone],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}
