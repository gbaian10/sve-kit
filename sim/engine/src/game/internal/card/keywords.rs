//! Current printed keyword authority and the CR 12.8.2 optional entry procedure.
use super::super::{DecisionId, Execution, InputDomain, ObjectRef, Wait};
use super::{Ability, Game, KeywordId, Located, Result, invalid, unsupported};
use crate::ir::lowering::PrintedKeyword;
use crate::ir::{Posture, PostureEvent, printed_keywords};
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
            match printed_keywords::card_declaration(&ability.node) {
                Some(KeywordId::Ward) => {
                    keywords.insert("guard".into());
                }
                Some(KeywordId::Aura) => {
                    keywords.insert("aura".into());
                }
                _ => {}
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
        let position = self
            .object_position(holder)?
            .and_then(|position| position.checked_add(1))
            .ok_or_else(|| invalid("Ward entry has no source zone position"))?;
        self.internal_wait(
            execution,
            DecisionId(0),
            execution.context.controller,
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

pub(super) fn check_coordinate(
    origins: &mut BTreeSet<(usize, u64, Option<KeywordId>)>,
    ordinal: usize,
    line: u64,
    keyword: Option<KeywordId>,
) -> Result<()> {
    if line == 0
        || !origins.insert((ordinal, line, keyword))
        || origins
            .iter()
            .any(|(other_face, other_line, other_keyword)| {
                *other_face == ordinal
                    && *other_line == line
                    && (keyword.is_none() != other_keyword.is_none())
            })
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
        Some("フォロワー" | "フォロワー・エボルヴ" | "フォロワー・アドバンス")
    ) {
        return Err(unsupported("printed keyword requires a follower face"));
    }
    let current = game
        .card_runtime
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
                (PrintedKeyword::Ward, KeywordId::Ward) | (PrintedKeyword::Aura, KeywordId::Aura)
            )
        })
        .ok_or_else(|| invalid("foreign printed keyword"))?;
    if located.source.author_path != format!("{}/keywords/{index}", proof.source.author_path) {
        return Err(invalid("printed keyword element source mismatch"));
    }
    Ok(())
}
