use super::super::{
    ActionPattern, Duration, DurationEnd, Effect, PermissionSubject, Polarity, RuleId, TimeAnchor,
};
use super::{
    Ability, AbilityId, AbilityKind, AbilityQualification, Bool, Expr, KeywordId, Located,
    LoweringError, ObjectOperand, ScopeId, Selector, ZoneKind, ability_source, ast, source_span,
    unsupported,
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
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    Ok(Located {
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
                condition: Expr::Literal(Bool(true)),
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
    })
}
