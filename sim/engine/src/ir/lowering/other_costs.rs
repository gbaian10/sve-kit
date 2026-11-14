use super::{
    Ability, AbilityId, Allocator, Binding, BodyContext, Cardinality, Expr, Located, LoweringError,
    Nat, PlayerRef, PlayerSet, ScopeId, SelectionView, Selector, TextQuantifier, ZoneKind, ast,
    unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, Action, CostChange, CostKind, Duration, DurationEnd, Effect,
    EffectReference, InstallationSubjects, InstallationTime, Int, Interval, TimeAnchor, UsageScope,
    UseLimit,
};
use core::mem::swap;

fn players(side: &ast::Side, path: &str) -> Result<PlayerSet, LoweringError> {
    Ok(PlayerSet {
        relation: side.relation().ok_or_else(|| unsupported(path))?,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum: None,
        },
        text_quantifier: TextQuantifier::Implicit,
    })
}

fn change(value: &ast::CostModifier, path: &str) -> Result<CostChange, LoweringError> {
    match (value.delta.as_ref(), value.set) {
        (Some(amount), None) => Ok(CostChange::Delta {
            amount: Expr::Literal(Int(super::decode(amount.clone(), path)?)),
        }),
        (None, Some(amount)) if amount >= 0 => Ok(CostChange::Set {
            amount: Expr::Literal(Int(amount)),
        }),
        _ => Err(unsupported(path)),
    }
}

fn query(
    value: &ast::PlayingQuery,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Selector, LoweringError> {
    let zones = match &value.zone {
        None => vec![
            ZoneKind::Hand,
            ZoneKind::Ex,
            ZoneKind::Cemetery,
            ZoneKind::Banish,
        ],
        Some(ast::PlayingZones::One(zone)) => vec![zone_kind(zone)],
        Some(ast::PlayingZones::Many(zones)) if !zones.is_empty() => {
            zones.iter().map(zone_kind).collect()
        }
        _ => return Err(unsupported(path)),
    };
    Ok(Selector::Union {
        sets: zones
            .into_iter()
            .map(|zone| {
                // Every alternative owns its candidate binding, including an implicit zone pair.
                let binding = Binding::new(1, allocator.scope(path)?);
                let predicate = super::selection::field_filter(
                    &super::selection::ObjectFilter {
                        kind: value.kind.as_ref().map(super::search::card_kind),
                        trait_id: value.r#trait.as_deref(),
                        all_traits: &value.traits_all,
                        class: value.class.as_deref(),
                        name: value.name.as_deref(),
                        original_cost: [None, value.orig_cost_le, None],
                        token: value.token,
                        ..Default::default()
                    },
                    binding,
                );

                Ok(Selector::Zone {
                    players: players(&value.who, path)?,
                    zone,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(predicate),
                })
            })
            .collect::<Result<_, LoweringError>>()?,
    })
}

const fn zone_kind(zone: &ast::PlayingZone) -> ZoneKind {
    match zone {
        ast::PlayingZone::Hand => ZoneKind::Hand,
        ast::PlayingZone::Ex => ZoneKind::Ex,
    }
}

fn selector(
    value: &ast::Playing,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Selector, LoweringError> {
    match value {
        ast::Playing::Query(value) => query(value, allocator, path),
        ast::Playing::Union { any_of } if !any_of.is_empty() => Ok(Selector::Union {
            sets: any_of
                .iter()
                .map(|member| query(member, allocator, path))
                .collect::<Result<_, _>>()?,
        }),
        ast::Playing::Union { .. } | ast::Playing::Affected(_) => Err(unsupported(path)),
    }
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let source = super::source_span(key, path, entry)?;
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
    if head.zone.is_some() || entry.imports.is_some() {
        return Err(unsupported(path));
    }
    let origin = super::ability_source(&source, None);
    let empty = ast::Body::default();
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    swap(&mut context.allocator, bindings);
    let condition = head
        .r#while
        .as_ref()
        .map(|value| context.qualification(value, path))
        .transpose()?;
    let mut declarations = Vec::new();
    for value in effects {
        if let ast::StaticEffect::EvolveCostMod(modifier) = value {
            if !matches!(modifier.on, ast::EvolutionCostSubject::This)
                || modifier.set.is_some()
                || head.r#while.is_some()
            {
                return Err(unsupported(path));
            }
            let argument = modifier.delta.as_ref().ok_or_else(|| unsupported(path))?;
            let number: ast::Number = super::decode(argument.clone(), path)?;
            let amount = context.number(&number, path, &mut super::BTreeSet::new())?;
            declarations.push(Effect::CostModifier {
                kind: CostKind::Evolution,
                subjects: super::numeric::holder(),
                change: CostChange::Delta { amount },
                duration: super::numeric::duration(None, path)?,
                usage: None,
            });
            continue;
        }
        let ast::StaticEffect::CostMod(value) = value else {
            return Err(unsupported(path));
        };
        if value.next_only.is_some() {
            return Err(unsupported(path));
        }
        let subjects = selector(&value.when_playing, context.card_allocator, path)?;
        let mut duration = super::numeric::duration(None, path)?;
        if let Some(predicate) = &condition {
            duration.end = DurationEnd::While {
                predicate: predicate.clone(),
            };
        }
        declarations.push(Effect::CostModifier {
            kind: CostKind::Play,
            subjects,
            change: change(value, path)?,
            duration,
            usage: None,
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player installations share the body binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn player_cost(
        &mut self,
        value: &ast::PlayerEffect,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::CostMod(modifier) = &value.effect else {
            return Err(unsupported(path));
        };
        let subjects = selector(&modifier.when_playing, self.card_allocator, path)?;
        let mut duration =
            if let Some(ast::Until::DuringNextTurn { during_next_turn }) = &value.until {
                let player = match during_next_turn.who {
                    ast::StandPlayer::Controller => PlayerRef::Controller {},
                    ast::StandPlayer::Opponent => super::super::events::timing::opponent(),
                };
                Duration {
                    end: DurationEnd::NextTurnEnd {
                        player: player.clone(),
                    },
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![Interval::DuringNextTurn { player }],
                }
            } else {
                super::numeric::duration(value.until.as_ref(), path)?
            };
        if modifier.next_only == Some(true) {
            duration.usage = Some(UseLimit {
                count: Nat(1),
                scope: UsageScope::SharedPlayer {
                    player: PlayerRef::Controller {},
                    effect: EffectReference::CurrentInstallation {},
                },
            });
        }
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Players {
                players: players(&value.who, path)?,
                affected: Binding::new(1, self.card_allocator.scope(path)?),
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                kind: CostKind::Play,
                subjects,
                change: change(modifier, path)?,
                duration: duration.clone(),
                usage: None,
            }),
            duration,
        })
    }
}
