use super::{BodyContext, LoweringError, SourceSpan, ast, keywords, numeric, unsupported};
use crate::ir::player_rules::next_turn_start;
use crate::ir::{
    Action, Duration, DurationEnd, FaceId, GrantedAbility, Interval, ObjectOperand, ObjectView,
    PlayerRef, Selector, TextProvider, TimeAnchor, ZoneKind,
};
use serde_json::{Value, json};
impl BodyContext<'_> {
    pub(super) fn grant(
        &mut self,
        value: &ast::Grant,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if (value.keywords.is_empty() && value.abilities.is_empty())
            || !matches!(
                value.until,
                None | Some(
                    ast::Until::Game
                        | ast::Until::Turn
                        | ast::Until::DuringNextTurn { .. }
                        | ast::Until::NextTurn {
                            next_turn: ast::TurnBegin {
                                who: ast::StandPlayer::Controller,
                                ..
                            }
                        }
                )
            )
        {
            return Err(unsupported(path));
        }
        let abilities = self.granted_abilities(&value.keywords, &value.abilities, source, path)?;
        let duration = if let Some(ast::Until::DuringNextTurn { during_next_turn }) = &value.until {
            let player = super::posture::player(during_next_turn.who);
            Duration {
                end: DurationEnd::NextTurnEnd {
                    player: player.clone(),
                },
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![Interval::DuringNextTurn { player }],
            }
        } else if matches!(value.until, Some(ast::Until::NextTurn { .. })) {
            Duration {
                end: next_turn_start(PlayerRef::Controller {}),
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            }
        } else {
            numeric::duration(value.until.as_ref(), path)?
        };
        Ok(Action::Grant {
            subjects: self.grant_subjects(&value.on, path)?,
            abilities,
            duration,
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId(source.face.clone()),
                ability: self.definition,
            },
        })
    }

    fn grant_subjects(&mut self, on: &ast::Objects, path: &str) -> Result<Selector, LoweringError> {
        if let ast::Objects::Zone(query) = on
            && matches!(query.zone, ast::Zone::Ex)
        {
            return self.zone_objects(query, ZoneKind::Ex, path);
        }
        if let ast::Objects::Follow { follow } = on {
            self.followed(follow, path)
        } else {
            self.objects(on, path)
        }
    }

    pub(super) fn granted_abilities(
        &mut self,
        keywords: &[String],
        definitions: &[Value],
        source: &SourceSpan,
        path: &str,
    ) -> Result<Vec<GrantedAbility>, LoweringError> {
        let mut abilities = keywords
            .iter()
            .map(|id| {
                keywords::grantable_keyword_id(id)
                    .map(|keyword| GrantedAbility::Keyword { keyword })
                    .ok_or_else(|| unsupported(path))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, head) in definitions.iter().enumerate() {
            let nested_path = format!("{path}/abilities/{index}");
            let mut entry = head.as_object().cloned().ok_or_else(|| unsupported(path))?;
            entry.insert("line".into(), source.line.into());
            entry.insert("ports".into(), json!({"imports":{},"exports":{}}));
            entry.insert("sources".into(), json!([self.source_use.json()]));
            let entry: ast::Ability = super::decode(Value::Object(entry), &nested_path)?;
            let scope = self.card_allocator.granted_scope(path)?;
            let mut nested_source = source.clone();
            nested_source.author_path.clone_from(&nested_path);
            let ability = if let Some(static_head) = &entry.r#static {
                let lower = match static_head.effects.first() {
                    // Per-grant conditions are admitted only for printed static abilities.
                    Some(ast::StaticEffect::Grant(_))
                        if !static_head
                            .effects
                            .iter()
                            .any(|effect| matches!(effect, ast::StaticEffect::If(_))) =>
                    {
                        super::static_keywords::lower_at_source
                    }
                    Some(
                        ast::StaticEffect::Prevent(_)
                        | ast::StaticEffect::DamageMod(_)
                        | ast::StaticEffect::DamageCap(_),
                    ) => super::prevention::lower_at_source,
                    Some(ast::StaticEffect::CostMod(_)) => super::other_costs::lower_at_source,
                    Some(ast::StaticEffect::MustAttack(_)) => {
                        super::attack_restrictions::lower_at_source
                    }
                    _ => return Err(unsupported(&nested_path)),
                };
                lower(
                    self.card_allocator,
                    &nested_path,
                    &entry,
                    scope,
                    nested_source,
                    &static_head.effects,
                    &mut super::Allocator::default(),
                )?
            } else if entry.play.is_some() {
                super::granted_play::lower(&entry, scope, nested_source, &nested_path)?
            } else {
                if entry.attack_time.is_none()
                    && entry.auto.is_none()
                    && entry.lastword.is_none()
                    && entry.activate.is_none()
                {
                    return Err(unsupported(&nested_path));
                }
                self.card_allocator.executable_ability(
                    super::hand_exit::AbilityInput::ordinary(&entry),
                    scope,
                    None,
                    None,
                    nested_source,
                    &nested_path,
                )?
            };
            abilities.push(GrantedAbility::Definition {
                ability: Box::new(ability.node),
            });
        }
        Ok(abilities)
    }
}
