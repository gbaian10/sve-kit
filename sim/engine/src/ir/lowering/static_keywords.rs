use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, keywords, numeric, source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, CharacteristicPatch, ContinuousLayer, Effect,
    EffectDefinitionId, EffectTarget, GrantedAbility, ObjectOperand, ScopeId, TimestampBasis,
    ZoneKind,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head.zone.as_ref().is_some_and(|zones| match zones {
        ast::ActivationZones::One(zone) => zone.kind() != ZoneKind::Field,
        ast::ActivationZones::Many(zones) => {
            !matches!(zones.as_slice(), [zone] if zone.kind() == ZoneKind::Field)
        }
    }) {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, &source, path))
        .transpose()?;
    let mut declarations = Vec::new();
    for effect in &head.effects {
        let ast::StaticEffect::Grant(grant) = effect;
        if grant.keywords.is_empty() {
            return Err(unsupported(path));
        }
        let abilities = grant
            .keywords
            .iter()
            .map(|name| {
                keywords::keyword_id(name)
                    .map(|keyword| GrantedAbility::Keyword { keyword })
                    .ok_or_else(|| unsupported(path))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let selector = grant
            .on
            .as_ref()
            .map_or_else(|| Ok(numeric::holder()), |on| context.objects(on, path))?;
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
            active_zones: vec![ZoneKind::Field],
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
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}
