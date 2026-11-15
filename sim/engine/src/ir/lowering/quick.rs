use super::super::{
    ActionPattern, Duration, DurationEnd, Effect, PermissionSubject, Polarity, RuleId, TimeAnchor,
};
use super::{
    Ability, AbilityId, AbilityKind, AbilityQualification, Allocator, BodyContext, Bool, Expr,
    KeywordId, Located, LoweringError, ObjectOperand, ScopeId, Selector, ZoneKind, ability_source,
    ast, source_span, unsupported,
};

pub(super) fn lower(
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    keywords: &[String],
) -> Result<Located<Ability>, LoweringError> {
    // Other keywords still require their own rule expansion; never discard them.
    if !matches!(keywords, [keyword] if keyword == "quick") {
        return Err(unsupported(&format!("{path}/keywords")));
    }
    let source = source_span(key, path, entry)?;
    Ok(permission(source, scope, Expr::Literal(Bool(true))))
}

pub(super) fn supports(head: &ast::Static) -> bool {
    let Some(ast::ActivationZones::Many(zones)) = &head.zone else {
        return false;
    };
    matches!(zones.as_slice(), [first, second]
        if matches!((first.kind(), second.kind()),
            (ZoneKind::Hand, ZoneKind::Ex) | (ZoneKind::Ex, ZoneKind::Hand)))
        && matches!(head.effects.as_slice(), [ast::StaticEffect::Grant(grant)]
            if matches!(grant.keywords.as_slice(), [keyword] if keyword == "quick")
                && grant.abilities.is_empty()
                && matches!(grant.on, None | Some(ast::Objects::This(_))))
}

pub(super) fn lower_static(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry
        .r#static
        .as_ref()
        .filter(|head| supports(head))
        .ok_or_else(|| unsupported(path))?;
    let empty = ast::Body::default();
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    Ok(permission(source_span(key, path, entry)?, scope, condition))
}

fn permission(
    source: super::SourceSpan,
    scope: ScopeId,
    condition: Expr<Bool>,
) -> Located<Ability> {
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    Located {
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: ability_source(&source, Some(KeywordId::Quick)),
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Hand, ZoneKind::Ex],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations: vec![Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: objects.clone(),
                },
                action: ActionPattern::PlayCard { objects },
                polarity: Polarity::Allow,
                condition,
                duration: Duration {
                    end: DurationEnd::NoTemporalExpiry {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                rule: RuleId(1232),
            }],
        },
        source,
    }
}
