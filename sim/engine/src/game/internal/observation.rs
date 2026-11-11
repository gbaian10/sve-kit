//! Source disclosure is captured at installation, not inferred from a later hidden location.

use crate::game::player::ObjectHandle;
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::ir::SourceSpan;

use super::{Execution, Game, Result};
use crate::game::player::observation::{EffectSource, seat};
use crate::ir::{AbilityOrigin, AbilitySource};
use serde::{Deserialize, Serialize};

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
    face: Option<String>,
    line: Option<u64>,
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
        let (card, face, line) = if execution.ability.is_some() {
            let provider = game.installed_effect_provider(execution)?;
            (
                provider.card,
                Some(provider.span.face),
                Some(u64::from(provider.span.line)),
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
            (game.information_source(&holder)?.card.clone(), face, line)
        };
        Ok(Self {
            link: ObjectLink::capture(game, &holder, execution.context.generation.0),
            card,
            face,
            line,
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
        source: &SourceSpan,
        publicly_identified: bool,
    ) -> Self {
        Self {
            link: ObjectLink::capture(game, holder, generation),
            card: source.card_no.clone(),
            face: Some(source.face.clone()),
            line: Some(u64::from(source.line)),
            publicly_identified,
        }
    }

    pub(super) fn observed(&self, game: &Game, perspective: Viewer) -> Option<EffectSource> {
        if self.link.period(perspective).is_none() && !self.publicly_identified {
            return None;
        }
        Some(EffectSource {
            card: self.card.clone(),
            face: self.face.clone(),
            line: self.line,
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
