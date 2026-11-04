use super::{BodyContext, LoweringError, SourceSpan, ast, keywords, numeric, unsupported};
use crate::ir::{
    Action, FaceId, GrantedAbility, ObjectOperand, ObjectView, Selector, TextProvider, ZoneKind,
};
use serde_json::{Value, json};
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword grants use the body's object bindings."
)]
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
                None | Some(ast::Until::Game | ast::Until::Turn)
            )
        {
            return Err(unsupported(path));
        }
        let abilities = self.granted_abilities(&value.keywords, &value.abilities, source, path)?;
        Ok(Action::Grant {
            subjects: self.grant_subjects(&value.on, path)?,
            abilities,
            duration: numeric::duration(value.until.as_ref(), path)?,
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
                    Some(ast::StaticEffect::Grant(_)) => super::static_keywords::lower_at_source,
                    Some(
                        ast::StaticEffect::Prevent(_)
                        | ast::StaticEffect::DamageMod(_)
                        | ast::StaticEffect::DamageCap(_),
                    ) => super::prevention::lower_at_source,
                    _ => return Err(unsupported(&nested_path)),
                };
                lower(
                    self.card_allocator,
                    &nested_path,
                    &entry,
                    scope,
                    nested_source,
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
                    &entry,
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
