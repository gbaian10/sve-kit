//! Menu options use the same declaration and payment stages as live play.

use super::super::grant::EffectiveAbility;
use super::{ActivationKey, CardPlay, Game, PlayerId, Result};
use crate::game::player::observation::{EffectSource, seat};
use crate::game::player::{ActivationOption, Viewer};
use crate::invalid;
use crate::ir::{AbilityKind, AbilityOrigin};

struct ActivationCandidate {
    ability: EffectiveAbility,
    source: Option<EffectSource>,
    line: u64,
}

pub(super) mod probe;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Main declarations share the existing whole-card authority."
)]
impl Game {
    pub(in crate::game) fn main_card_declaration_available(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        if !self.card_runtime.is_loaded(&object.card)
            || !self.card_play_permission(holder, actor)?
        {
            return Ok(false);
        }
        let kind = self.object_type(holder)?;
        let field_card = matches!(
            kind,
            "フォロワー"
                | "アミュレット"
                | "フォロワー・トークン"
                | "アミュレット・トークン"
                | "フォロワー・アドバンス"
        );
        if !field_card {
            if !matches!(kind, "スペル" | "スペル・トークン" | "スペル・アドバンス")
            {
                return Ok(false);
            }
            if kind != "スペル・トークン"
                && !self
                    .internal_printed_abilities(holder)?
                    .iter()
                    .any(|entry| entry.node.kind == AbilityKind::Spell)
            {
                return Ok(false);
            }
        }
        self.check_card_play_context(holder, actor)?;
        let mut probe = self.declaration_probe();
        let execution = probe.prepare_permitted_card_execution(holder, actor, None);
        probe.declaration_payable(execution)
    }

    fn main_activation_candidates(
        &self,
        holder: &str,
        actor: Viewer,
    ) -> Result<Vec<ActivationCandidate>> {
        let object = self.object(holder)?;
        if !self.card_runtime.is_loaded(&object.card) || object.controller != seat(actor) {
            return Ok(vec![]);
        }
        let mut candidates = Vec::new();
        for ability in self.internal_effective_abilities(holder)? {
            if ability.node.kind != AbilityKind::Activated
                || super::evolution::intrinsic(&ability.node)
                || !self.activation_holder_available(holder, &ability, actor.player())?
            {
                continue;
            }
            let line = if let Some(origin) = &ability.grant {
                match &ability.node.source.origin {
                    AbilityOrigin::Printed { line, .. } => line.0,
                    _ => u64::from(origin.provider().span.line),
                }
            } else {
                super::printed_origin(&ability.node.source.origin)?.1.0
            };
            let source = self.activation_text_source(holder, &ability, actor)?;
            candidates.push(ActivationCandidate {
                ability,
                source,
                line,
            });
        }
        candidates.sort_by_key(|candidate| {
            (
                candidate.source.as_ref().map(|source| {
                    (
                        source.card.clone(),
                        source.face,
                        source.line,
                        source.holder.clone(),
                    )
                }),
                candidate.line,
            )
        });
        Ok(candidates)
    }

    pub(in crate::game) fn main_activation_options(
        &self,
        holder: &str,
        actor: Viewer,
    ) -> Result<Vec<ActivationOption>> {
        let mut options = Vec::new();
        for (ordinal, candidate) in self
            .main_activation_candidates(holder, actor)?
            .into_iter()
            .enumerate()
        {
            let mut probe = self.declaration_probe();
            let source = probe.internal_effective_source(holder, &candidate.ability)?;
            let execution = probe.prepare_effective_activation(
                holder,
                candidate.ability,
                actor.player(),
                source,
            );
            if probe.declaration_payable(execution)? {
                options.push(ActivationOption {
                    ordinal: u64::try_from(ordinal).map_err(invalid)?,
                    source: candidate.source,
                    line: candidate.line,
                });
            }
        }
        Ok(options)
    }

    pub(in crate::game) fn activate_main_option(
        &mut self,
        holder: &str,
        ordinal: u64,
        actor: Viewer,
        declared: &mut bool,
    ) -> Result<CardPlay> {
        let selected = self
            .main_activation_candidates(holder, actor)?
            .into_iter()
            .nth(usize::try_from(ordinal).map_err(invalid)?)
            .ok_or_else(|| invalid("retired main activation option"))?;
        self.activate_internal_entry(
            holder,
            &ActivationKey::Effective {
                definition: selected.ability.node.id,
                grant: selected.ability.grant.map(Box::new),
            },
            actor.player(),
            declared,
        )
    }
}
