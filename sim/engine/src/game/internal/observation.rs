//! Source disclosure is captured at installation, not inferred from a later hidden location.

use crate::game::player::ObjectHandle;
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::ir::SourceSpan;

use super::{Execution, Game, Result};
use crate::game::player::observation::{EffectSource, WordingField, WordingSource, seat};
use crate::ir::{AbilityOrigin, AbilitySource};
use serde::{Deserialize, Serialize};

/// Accepts only spans already verified by Current/direct admission.
/// Shape checks cannot establish a revision's face or printing ownership.
pub(super) fn wording_source(source: &SourceSpan) -> Option<Box<WordingSource>> {
    let owner: serde_json::Value = serde_json::from_str(&source.owner).ok()?;
    let face = source.face.strip_prefix("f:")?;
    if owner["kind"] != "face_revision"
        || face.len() != 32
        || !face.bytes().all(|byte| byte.is_ascii_hexdigit())
        || source.line == 0
    {
        return None;
    }
    let revision = owner["revision_id"].as_str().filter(|id| !id.is_empty())?;
    let field = match (source.field.as_str(), source.section.as_deref()) {
        ("effect", None) => WordingField::Effect,
        ("section", Some(section)) => {
            let ordinal = section.parse::<u64>().ok()?;
            if ordinal.to_string() != section {
                return None;
            }
            WordingField::Section { ordinal }
        }
        _ => return None,
    };
    Some(Box::new(WordingSource {
        face_id: source.face.clone(),
        revision_id: revision.into(),
        field,
    }))
}

#[cfg(test)]
mod wording_tests {
    use super::*;
    use serde_json::json;

    fn span() -> SourceSpan {
        SourceSpan {
            card_id: "synthetic".into(),
            card_no: "synthetic-printing".into(),
            face: "f:0123456789abcdef0123456789abcdef".into(),
            owner: json!({"kind":"face_revision","revision_id":"revision-synthetic"}).to_string(),
            field: "effect".into(),
            section: None,
            line: 1,
            scope: "private-scope".into(),
            fragment: Some("private-fragment".into()),
            author_path: "/private-author-coordinate".into(),
        }
    }

    #[test]
    fn wording_source_keeps_the_checked_owner_and_printed_field() {
        let mut source = span();
        let mut expected = Box::new(WordingSource {
            face_id: source.face.clone(),
            revision_id: "revision-synthetic".into(),
            field: WordingField::Effect,
        });
        assert_eq!(wording_source(&source), Some(expected.clone()));
        for ordinal in [0, 2, u64::MAX] {
            source.field = "section".into();
            source.section = Some(ordinal.to_string());
            expected.field = WordingField::Section { ordinal };
            assert_eq!(wording_source(&source), Some(expected.clone()));
        }
    }

    #[test]
    fn wording_source_never_guesses_legacy_or_incomplete_coordinates() {
        for (field, value) in [
            ("owner", "card"),
            ("owner", r#"{"kind":"face_revision"}"#),
            ("owner", r#"{"kind":"face_revision","revision_id":""}"#),
            (
                "owner",
                r#"{"kind":"printing_face","revision_id":"revision-synthetic"}"#,
            ),
            ("face", "front"),
            ("face", "f:incomplete"),
            ("field", "text"),
            ("line", "0"),
        ] {
            let mut source = span();
            match field {
                "owner" => source.owner = value.into(),
                "face" => source.face = value.into(),
                "field" => source.field = value.into(),
                "line" => source.line = 0,
                _ => panic!("unsupported synthetic source field"),
            }
            assert_eq!(wording_source(&source), None);
        }
        for section in [
            None,
            Some("-1"),
            Some("00"),
            Some("+1"),
            Some("18446744073709551616"),
        ] {
            let mut source = span();
            source.field = "section".into();
            source.section = section.map(String::from);
            assert_eq!(wording_source(&source), None);
        }
        let mut source = span();
        source.section = Some("0".into());
        assert_eq!(wording_source(&source), None);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ObjectLink {
    holder: String,
    generation: u64,
    periods: [Option<u64>; 2],
}

impl ObjectLink {
    pub(super) fn capture(game: &Game, holder: &str, generation: u64) -> Self {
        let period = |viewer| {
            game.state
                .knowledge
                .get(seat(viewer))
                .and_then(|knowledge| knowledge.identity_period(holder))
        };
        Self {
            holder: holder.into(),
            generation,
            periods: [period(Viewer::P1), period(Viewer::P2)],
        }
    }

    pub(super) fn reference(game: &Game, number: u64, generation: u64) -> Option<Self> {
        let (holder, _) = game
            .state
            .ir_objects
            .iter()
            .find(|(_, value)| **value == number)?;
        Some(Self::capture(game, holder, generation))
    }

    fn period(&self, perspective: Viewer) -> Option<u64> {
        match perspective {
            Viewer::P1 => self.periods.first(),
            Viewer::P2 => self.periods.get(1),
        }
        .copied()
        .flatten()
    }

    pub(super) fn observed(&self, game: &Game, perspective: Viewer) -> Option<ObjectHandle> {
        let original = self.period(perspective)?;
        let knowledge = game.state.knowledge.get(seat(perspective))?;
        let object = game.state.objects.get(&self.holder)?;
        (knowledge.identity_period(&self.holder) == Some(original)
            && object.generation == self.generation
            && game.player_knows_object(perspective, &self.holder))
        .then(|| knowledge.handle(&self.holder))
        .flatten()
    }

    pub(super) fn observed_public(&self, game: &Game, perspective: Viewer) -> Option<ObjectHandle> {
        let object = game.state.objects.get(&self.holder)?;
        if !matches!(
            object.zone.as_str(),
            "field"
                | "ex"
                | "cemetery"
                | "banish"
                | "resolution"
                | "evolution"
                | "equipment"
                | "race"
                | "drive"
                | "trigger"
        ) {
            return None;
        }
        // Origin rollback can restore generation while published knowledge keeps a new period.
        self.observed(game, perspective)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct EffectOrigin {
    link: ObjectLink,
    card: String,
    face: Option<u64>,
    line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wording: Option<Box<WordingSource>>,
    publicly_identified: bool,
}

impl EffectOrigin {
    pub(super) fn holder_link(&self) -> ObjectLink {
        self.link.clone()
    }

    pub(super) fn capture(
        game: &Game,
        execution: &Execution,
        source: &AbilitySource,
    ) -> Result<Self> {
        let holder = execution.context.source.clone();
        let (card, face, line, wording) = if execution.ability.is_some() {
            let provider = game.installed_effect_provider(execution)?;
            let wording = wording_source(&provider.span);
            (
                provider.card,
                Some(provider.span.face),
                Some(u64::from(provider.span.line)),
                wording,
            )
        } else {
            let line = match source.origin {
                AbilityOrigin::Printed { line, .. } => Some(line.0),
                _ => None,
            };
            let face = match &source.origin {
                AbilityOrigin::Printed { face, .. } => Some(face.0.clone()),
                _ => None,
            };
            (
                game.information_source(&holder)?.card.clone(),
                face,
                line,
                None,
            )
        };
        Ok(Self {
            link: ObjectLink::capture(game, &holder, execution.context.generation.0),
            face: game.effect_source_face(&card, face.as_deref())?,
            card,
            line,
            wording,
            publicly_identified: execution
                .source_information
                .as_ref()
                .is_some_and(super::source::SourceInformation::publicly_identified),
        })
    }

    pub(super) fn definition(
        game: &Game,
        holder: &str,
        generation: u64,
        card: &str,
        source: &SourceSpan,
        publicly_identified: bool,
    ) -> Result<Self> {
        Ok(Self {
            link: ObjectLink::capture(game, holder, generation),
            card: card.into(),
            face: game.effect_source_face(card, Some(&source.face))?,
            line: Some(u64::from(source.line)),
            wording: wording_source(source),
            publicly_identified,
        })
    }

    pub(super) fn observed(&self, game: &Game, perspective: Viewer) -> Option<EffectSource> {
        if self.link.period(perspective).is_none() && !self.publicly_identified {
            return None;
        }
        Some(EffectSource {
            card: self.card.clone(),
            face: self.face,
            line: self.line,
            wording: self.wording.clone(),
            holder: self.link.observed(game, perspective),
        })
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed effect observations share their authoritative providers."
)]
impl Game {
    pub(in crate::game) fn observed_player_effects(
        &self,
        perspective: Viewer,
        player: Viewer,
    ) -> Result<Vec<InstalledEffect>> {
        let mut effects = self.observed_leader_preventions(perspective, player)?;
        effects.extend(self.observed_player_costs(perspective, player)?);
        effects.extend(self.observed_delayed(perspective, player));
        effects.extend(self.observed_banish_permission(perspective, player));
        Ok(effects)
    }

    pub(in crate::game) fn observed_installed_effects(
        &self,
        perspective: Viewer,
        holder: &str,
    ) -> Result<Vec<InstalledEffect>> {
        let mut result = self.observed_numeric_effects(perspective, holder)?;
        result.extend(self.observed_cost_effects(perspective, holder)?);
        result.extend(self.observed_grant_effects(perspective, holder)?);
        result.extend(self.observed_suppression_effects(perspective, holder)?);
        result.extend(self.observed_stand_locks(perspective, holder)?);
        result.extend(self.observed_drive_locks(perspective, holder)?);
        result.extend(self.observed_attack_locks(perspective, holder)?);
        result.extend(self.observed_ability_locks(perspective, holder)?);
        result.extend(self.observed_card_preventions(perspective, holder)?);
        Ok(result)
    }
}
