use super::{
    Ability, AbilityId, Allocator, BodyContext, CardCoordinates, Located, LoweringError,
    ability_source, ast, numeric, source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, CharacteristicPatch, ContinuousLayer, Effect,
    EffectDefinitionId, EffectTarget, ObjectOperand, ScopeId, TimestampBasis, ZoneKind,
};
use crate::ir::{Bool, Expr, Predicate};
use crate::ir::{GrantedAbility, KeywordId, ObjectRelation};
use crate::ir::{ObjectView, Selector};
use core::mem::swap;

pub(super) fn grant(
    effect: &ast::StaticEffect,
) -> Option<(&ast::StaticGrant, Option<&ast::Condition>)> {
    if let ast::StaticEffect::Grant(grant) = effect {
        return Some((grant, None));
    }
    if let ast::StaticEffect::If(branch) = effect
        && let [ast::StaticConditionalEffect::Grant(grant)] = &branch.then
    {
        return Some((grant, Some(&branch.cond)));
    }
    None
}

fn combined(head: Option<&Expr<Bool>>, branch: Option<Expr<Bool>>) -> Option<Expr<Bool>> {
    match (head, branch) {
        (Some(head), Some(branch)) => Some(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![head.clone(), branch],
        }))),
        (Some(head), None) => Some(head.clone()),
        (None, branch) => branch,
    }
}

fn active_zones(head: &ast::Static, path: &str) -> Result<Vec<ZoneKind>, LoweringError> {
    let zones = match head.zone.as_ref() {
        Some(ast::ActivationZones::Many(zones))
            if matches!(zones.as_slice(),
            [first, second] if matches!((first.kind(), second.kind()),
                (ZoneKind::Field, ZoneKind::Ex) | (ZoneKind::Ex, ZoneKind::Field))) =>
        {
            vec![ZoneKind::Field, ZoneKind::Ex]
        }
        zones => vec![
            zones
                .map_or(Some(ZoneKind::Field), ast::ActivationZones::single)
                .ok_or_else(|| unsupported(path))?,
        ],
    };
    let equipment = zones == [ZoneKind::Equipment];
    if !matches!(
        zones.as_slice(),
        [ZoneKind::Field | ZoneKind::Equipment] | [ZoneKind::Field, ZoneKind::Ex]
    ) || ((equipment || zones.len() > 1) && head.r#while.is_some())
    {
        return Err(unsupported(path));
    }
    Ok(zones)
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let source = source_span(key, path, entry)?;
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    lower_at_source(
        allocator,
        path,
        entry,
        scope,
        source,
        &head.effects,
        &mut Allocator::default(),
    )
}

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
    let zones = active_zones(head, path)?;
    let equipment = zones == [ZoneKind::Equipment];
    let origin = ability_source(&source, None);
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    swap(&mut context.allocator, bindings);
    let head_condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?;
    let mut declarations = Vec::new();
    for effect in effects {
        let Some((grant, branch)) = grant(effect) else {
            return Err(unsupported(path));
        };
        let condition = combined(
            head_condition.as_ref(),
            branch
                .map(|predicate| context.qualification(predicate, path))
                .transpose()?,
        );
        if (equipment || zones.len() > 1) && condition.is_some() {
            return Err(unsupported(path));
        }
        if grant.keywords.is_empty() && grant.abilities.is_empty() {
            return Err(unsupported(path));
        }
        let abilities =
            context.granted_abilities(&grant.keywords, &grant.abilities, &source, path)?;
        if zones.len() > 1
            && !matches!(
                abilities.as_slice(),
                [GrantedAbility::Keyword {
                    keyword: KeywordId::Rush
                }]
            )
        {
            return Err(unsupported(path));
        }
        let selector = if equipment {
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
            active_zones: zones.clone(),
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
            active_zones: zones,
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}
