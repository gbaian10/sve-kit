//! Declaration inspection never runs payment, random actions or effect bodies.

use super::super::grant::EffectiveAbility;
use super::{ActivationKey, CardPlay, Execution, Game, PlayerId, Result, int, seat_name};
use crate::game::player::observation::{EffectSource, seat};
use crate::game::player::{ActivationOption, Viewer};
use crate::ir::{
    AbilityKind, AbilityOrigin, Action, CostSpec, Expr, Holder, ObjectOperand, Selector,
};
use crate::{EngineFailure, invalid};

struct ActivationCandidate {
    ability: EffectiveAbility,
    source: Option<EffectSource>,
    line: u64,
}

fn source_only(objects: &Selector) -> bool {
    matches!(objects, Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
}

fn fixed_head(costs: &CostSpec) -> bool {
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    steps.iter().all(|step| match &step.instruction.action {
        Action::Resource {
            amount: Expr::Literal(_),
            ..
        }
        | Action::Life { .. }
        | Action::Counter {
            holder:
                Holder::Object {
                    object: ObjectOperand::SourceHolder {},
                },
            amount: Expr::Literal(_),
            ..
        } => true,
        Action::Posture { objects, .. } | Action::Banish { objects } => source_only(objects),
        Action::Discard { .. } => super::hand_payment::source_discard(&step.instruction.action),
        _ => false,
    })
}

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
            "フォロワー" | "アミュレット" | "フォロワー・トークン" | "アミュレット・トークン"
        );
        if !field_card {
            if !matches!(kind, "スペル" | "スペル・トークン" | "スペル・アドバンス")
            {
                return Ok(false);
            }
            let entries = self.internal_printed_abilities(holder)?;
            if !entries
                .iter()
                .any(|entry| entry.node.kind == AbilityKind::Spell)
            {
                return Ok(false);
            }
        }
        let mut probe = self.clone();
        match probe.prepare_card_execution(holder, actor, None) {
            Ok(execution) => probe.declaration_cost_available(holder, &execution),
            Err(EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn declaration_cost_available(&self, holder: &str, execution: &Execution) -> Result<bool> {
        let play = execution
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing prepared declaration"))?;
        let available_pp =
            int(&self.player(seat_name(execution.context.controller)?)?.pp["current"]);
        let cost = self
            .prepared_declaration_cost(execution)
            .ok_or_else(|| invalid("missing prepared cost"))?;
        if execution.field_entry
            && play.exchange.is_none()
            && cost.pending.is_empty()
            && fixed_head(&play.costs)
            && play.additional.is_none()
            && play.optional.is_empty()
            && self.zone_count(seat_name(execution.context.controller)?, "field") >= 5
        {
            return Ok(false);
        }
        if cost.total_pp.is_some_and(|amount| amount > available_pp) {
            return Ok(false);
        }
        if cost.pending.is_empty()
            && play.exchange.is_none()
            && fixed_head(&play.costs)
            && self.known_costs_available(
                holder,
                execution,
                &play.costs,
                execution.context.controller,
                execution.spell,
            )? == Some(false)
        {
            return Ok(false);
        }
        Ok(true)
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
            let instance = self.cached_effective_instance(holder, &ability)?;
            let context =
                match self.check_internal_activation(holder, &ability, actor.player(), instance) {
                    Ok(context) => context,
                    Err(EngineFailure::IllegalPlay(_)) => continue,
                    Err(error) => return Err(error),
                };
            let mut probe = self.clone();
            let runtime_source = probe.internal_effective_source(holder, &ability)?;
            let execution = match probe.prepare_activated_execution(
                ability.definition.clone(),
                context,
                runtime_source,
                false,
            ) {
                Ok(execution) => execution,
                Err(EngineFailure::IllegalPlay(_)) => continue,
                Err(error) => return Err(error),
            };
            if !probe.declaration_cost_available(holder, &execution)? {
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
                        source.face.clone(),
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
        self.main_activation_candidates(holder, actor)?
            .into_iter()
            .enumerate()
            .map(|(ordinal, candidate)| {
                Ok(ActivationOption {
                    ordinal: u64::try_from(ordinal).map_err(invalid)?,
                    source: candidate.source,
                    line: candidate.line,
                })
            })
            .collect()
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
