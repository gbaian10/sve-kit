use super::{
    Ability, AbilityId, Allocator, CardCoordinates, Located, LoweringError, ability_source, ast,
    numeric, source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, CardName, CharacteristicPatch, ContinuousLayer, Effect,
    EffectDefinitionId, EffectTarget, ObjectOperand, ScopeId, TimestampBasis, ZoneKind, names,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let source = source_span(key, path, entry)?;
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::AlsoNamed(alias)] = head.effects.as_slice() else {
        return Err(unsupported(path));
    };
    if head.r#while.is_some()
        || alias.name.is_empty()
        || alias
            .on
            .as_ref()
            .is_some_and(|on| !matches!(on, ast::Objects::Bound(name) if name == "this"))
    {
        return Err(unsupported(path));
    }
    let zones = match &head.zone {
        None => names::ALL_ZONES.to_vec(),
        Some(zone) if zone.single() == Some(ZoneKind::Field) => vec![ZoneKind::Field],
        Some(_) => return Err(unsupported(path)),
    };
    let origin = ability_source(&source, None);
    let effect = Effect::Continuous {
        id: EffectDefinitionId(allocator.id(path)?),
        source: origin.clone(),
        target: EffectTarget::Dynamic {
            selector: numeric::holder(),
        },
        condition: None,
        patch: CharacteristicPatch::AddName {
            name: CardName(alias.name.clone()),
        },
        layer: ContinuousLayer::NonNumeric,
        timestamp: TimestampBasis::ZoneEntry {
            object: ObjectOperand::SourceHolder {},
        },
        active_zones: zones.clone(),
        duration: numeric::duration(None, path)?,
    };
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: zones,
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations: vec![effect],
        },
    })
}
