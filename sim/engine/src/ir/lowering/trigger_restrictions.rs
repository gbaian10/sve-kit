use super::{
    Ability, AbilityId, Allocator, CardCoordinates, Located, LoweringError, ability_source, ast,
    source_span, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityPredicate, AbilityQualification, ActionPattern, Bool, Cardinality, Effect,
    Expr, KeywordId, Nat, PermissionSubject, PlayerRef, PlayerRelation, PlayerSet, Polarity,
    RuleId, ScopeId, TextQuantifier, ZoneKind,
};

pub(super) fn lower(
    _allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head.r#while.is_some()
        || head.zone.as_ref().is_some_and(|zones| match zones {
            ast::ActivationZones::One(zone) => zone.kind() != ZoneKind::Field,
            ast::ActivationZones::Many(zones) => {
                !matches!(zones.as_slice(), [zone] if zone.kind() == ZoneKind::Field)
            }
        })
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let mut declarations = Vec::new();
    for effect in &head.effects {
        let ast::StaticEffect::NoTrigger(shape) = effect else {
            return Err(unsupported(path));
        };
        if !matches!(shape.who, Some(ast::EntryPlayer::Opponent))
            || shape.on.is_some()
            || shape.kinds.is_empty()
        {
            return Err(unsupported(path));
        }
        for kind in &shape.kinds {
            let keyword = match kind {
                ast::NoTriggerKind::Fanfare => KeywordId::Fanfare,
                ast::NoTriggerKind::EvolveTime => KeywordId::OnEvolve,
            };
            declarations.push(Effect::Permission {
                subject: PermissionSubject::Players {
                    players: PlayerSet {
                        relation: PlayerRelation::Opponents,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(0),
                            maximum: None,
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                },
                action: ActionPattern::TriggerAdmission {
                    predicate: AbilityPredicate {
                        kind: Some(AbilityKind::Triggered),
                        keyword: Some(keyword),
                        except: vec![],
                        holder: None,
                        origin: None,
                    },
                },
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: super::numeric::duration(None, path)?,
                rule: RuleId(133),
            });
        }
    }
    Ok(Located {
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: ability_source(&source, None),
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
        source,
    })
}
