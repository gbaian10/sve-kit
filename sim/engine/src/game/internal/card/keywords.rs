//! Current printed keyword authority and the CR 12.8.2 optional entry procedure.
use super::super::{DecisionId, Execution, InputDomain, ObjectRef, Wait, grant, player_id};
use super::{Ability, Game, KeywordId, Located, Result, invalid, unsupported};
use crate::ir::lowering::PrintedKeyword;
use crate::ir::{Posture, PostureEvent, SourceSpan, printed_keywords};
use alloc::collections::BTreeSet;
use core::slice::from_ref;
use serde_json::Value;
use serde_json::json;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword availability and entry choices share the admitted printed definitions."
)]
impl Game {
    pub(in crate::game) fn internal_keywords(
        &self,
        holder: &str,
    ) -> Result<Option<BTreeSet<String>>> {
        if !self.card_runtime.is_empty() {
            self.require_internal_abilities(holder, "keyword availability")?;
            self.require_ability_sources("keyword availability")?;
        }
        let object = self.information_source(holder)?;
        if !self.card_runtime.is_loaded(&object.card) {
            return Ok(None);
        }
        let mut keywords = BTreeSet::new();
        for ability in self.internal_printed_abilities(holder)? {
            if let Some(keyword) =
                printed_keywords::card_declaration(&ability.node).and_then(grant::keyword)
            {
                keywords.insert(keyword.into());
            }
        }
        Ok(Some(keywords))
    }

    pub(in crate::game::internal) fn internal_ward_entry_choice(
        &mut self,
        execution: &mut Execution,
        holder: &str,
    ) -> Result<bool> {
        let reference = execution.reference(self, holder)?;
        if execution
            .ward_entries
            .iter()
            .any(|(object, _)| *object == reference)
        {
            return Ok(false);
        }
        let previous = self.object(holder)?;
        let face = Self::movement_face(previous)?;
        let printed = self.catalog.face(&previous.card, face)?;
        let state = Self::moved_attributes(previous, printed, "field");
        // CR 12.8.2 uses the arriving information, including a reset of old-zone ability loss.
        let mut prospective = self.clone();
        let arrived = prospective.object_mut(holder)?;
        arrived.zone = "field".into();
        arrived.state = state;
        if prospective.object(holder)?.state["acted"] == true
            || !prospective.keywords(holder)?.contains("guard")
        {
            return Ok(false);
        }
        let position = if previous.zone == "void" && previous.generation == 0 {
            1
        } else {
            self.object_position(holder)?
                .and_then(|position| position.checked_add(1))
                .ok_or_else(|| invalid("Ward entry has no source zone position"))?
        };
        self.internal_wait(
            execution,
            DecisionId(0),
            player_id(&previous.controller)?,
            InputDomain::WardEntry {
                card: previous.card.clone(),
                name: prospective.card_name(holder)?,
                zone: previous.zone.clone(),
                position,
            },
            Wait::WardEntry(reference),
        )?;
        Ok(true)
    }

    pub(in crate::game::internal) fn internal_ward_entry_posture(
        &mut self,
        execution: &mut Execution,
        before: ObjectRef,
    ) -> Result<()> {
        if let Some(index) = execution
            .ward_entries
            .iter()
            .position(|(object, _)| *object == before)
        {
            let (_, acted) = execution.ward_entries.remove(index);
            if acted {
                let id = execution.object_id(before)?.to_owned();
                if !self.keywords(&id)?.contains("guard") {
                    return Err(invalid("Ward is unavailable after entry"));
                }
                self.change_posture(from_ref(&id), true, &json!({"rule":"12.8.2"}))?;
                execution.facts.push(super::Fact::Posture(PostureEvent {
                    object: execution.reference(self, &id)?,
                    before: Posture::Standing,
                    after: Posture::Acted,
                }));
            }
        }
        Ok(())
    }
}

pub(super) type PrintedCoordinate<'source> = (
    usize,
    &'source str,
    Option<&'source str>,
    u64,
    &'source str,
    Option<KeywordId>,
);

pub(super) fn check_coordinate<'source>(
    origins: &mut BTreeSet<PrintedCoordinate<'source>>,
    ordinal: usize,
    source: &'source SourceSpan,
    keyword: Option<KeywordId>,
) -> Result<()> {
    let line = u64::from(source.line);
    let scope = source.scope.as_str();
    if line == 0
        || !origins.insert((
            ordinal,
            source.field.as_str(),
            source.section.as_deref(),
            line,
            scope,
            keyword,
        ))
        || origins.iter().any(
            |(other_face, other_field, other_section, other_line, other_scope, other_keyword)| {
                *other_face == ordinal
                    && *other_field == source.field
                    && *other_section == source.section.as_deref()
                    && *other_line == line
                    && *other_scope == scope
                    && (keyword.is_none() != other_keyword.is_none())
            },
        )
    {
        return Err(invalid("invalid or duplicate printed ability coordinate"));
    }
    Ok(())
}

pub(super) fn check(
    game: &Game,
    card: &str,
    located: &Located<Ability>,
    printed: &Value,
    keyword: KeywordId,
) -> Result<()> {
    if !matches!(
        printed["card_type"].as_str(),
        Some(
            "フォロワー"
                | "フォロワー・エボルヴ"
                | "フォロワー・アドバンス"
                | "フォロワー・トークン"
        )
    ) {
        return Err(unsupported("printed keyword requires a follower face"));
    }
    let current = game
        .card_runtime
        .definitions
        .current
        .get(card)
        .ok_or_else(|| unsupported("printed keyword requires current declaration proof"))?;
    let proof = current
        .candidate
        .declaration_at(&located.source.author_path)
        .ok_or_else(|| invalid("missing printed keyword reference"))?;
    let index = proof
        .keywords()
        .iter()
        .position(|item| {
            matches!(
                (item, keyword),
                (PrintedKeyword::Ward, KeywordId::Ward)
                    | (PrintedKeyword::Aura, KeywordId::Aura)
                    | (PrintedKeyword::Storm, KeywordId::Storm)
                    | (PrintedKeyword::Rush, KeywordId::Rush)
                    | (PrintedKeyword::Bane, KeywordId::Bane)
                    | (PrintedKeyword::Intimidate, KeywordId::Intimidate)
                    | (PrintedKeyword::Assail, KeywordId::Assail)
            )
        })
        .ok_or_else(|| invalid("foreign printed keyword"))?;
    if located.source.author_path != format!("{}/keywords/{index}", proof.source.author_path) {
        return Err(invalid("printed keyword element source mismatch"));
    }
    Ok(())
}
