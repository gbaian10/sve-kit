//! Source disclosure is captured at installation, not inferred from a later hidden location.

use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;

use super::{Execution, Game, Result};
use crate::game::player::observation::{EffectSource, seat};
use crate::ir::{AbilityOrigin, AbilitySource};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct EffectOrigin {
    holder: String,
    generation: u64,
    card: String,
    face: Option<String>,
    line: Option<u64>,
    links: [Option<u64>; 2],
    publicly_identified: bool,
}

impl EffectOrigin {
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
        let identity_mark = |viewer| {
            game.state
                .knowledge
                .get(seat(viewer))
                .and_then(|knowledge| knowledge.identity_period(&holder))
        };
        let links = [identity_mark(Viewer::P1), identity_mark(Viewer::P2)];
        Ok(Self {
            holder,
            generation: execution.context.generation.0,
            card,
            face,
            line,
            links,
            publicly_identified: execution
                .source_information
                .as_ref()
                .is_some_and(super::source::SourceInformation::publicly_identified),
        })
    }

    pub(super) fn observed(&self, game: &Game, perspective: Viewer) -> Option<EffectSource> {
        let original = match perspective {
            Viewer::P1 => self.links.first(),
            Viewer::P2 => self.links.get(1),
        }
        .copied()
        .flatten();
        if original.is_none() && !self.publicly_identified {
            return None;
        }

        let knowledge = game.state.knowledge.get(seat(perspective))?;
        Some(EffectSource {
            card: self.card.clone(),
            face: self.face.clone(),
            line: self.line,
            holder: knowledge.handle(&self.holder).filter(|_| {
                original.is_some()
                    && knowledge.identity_period(&self.holder) == original
                    && game
                        .state
                        .objects
                        .get(&self.holder)
                        .is_some_and(|object| object.generation == self.generation)
                    && game.player_knows_object(perspective, &self.holder)
            }),
        })
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Installed effect observations share their authoritative providers."
)]
impl Game {
    pub(in crate::game) fn observed_installed_effects(
        &self,
        perspective: Viewer,
        holder: &str,
    ) -> Result<Vec<InstalledEffect>> {
        let mut result = self.observed_numeric_effects(perspective, holder)?;
        result.extend(self.observed_cost_effects(perspective, holder)?);
        result.extend(self.observed_grant_effects(perspective, holder)?);
        result.extend(self.observed_suppression_effects(perspective, holder)?);
        Ok(result)
    }
}
