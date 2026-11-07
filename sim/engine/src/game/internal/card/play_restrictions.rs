use super::{Ability, Context, Game, Generation, Located, PlayerId, Result, unsupported};
use crate::ir::play_restrictions::card_play_rule;
use crate::ir::{Bool, Expr, ZoneKind};

pub(super) fn check(located: &Located<Ability>, ordinal: usize) -> Result<bool> {
    let Some(rule) = card_play_rule(&located.node) else {
        return Ok(false);
    };
    if ordinal != 0 {
        return Err(unsupported("play restriction requires base face"));
    }
    super::super::ordering::check_condition(rule.condition, &located.source)?;
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Printed restrictions share the authoritative card play boundary."
)]
impl Game {
    pub(super) fn internal_card_play_forbidden(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        self.internal_card_play_rule_matches(holder, actor, true)
    }

    pub(super) fn internal_card_cemetery_play_allowed(
        &self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<bool> {
        self.internal_card_play_rule_matches(holder, actor, false)
    }

    fn internal_card_play_rule_matches(
        &self,
        holder: &str,
        actor: PlayerId,
        forbids: bool,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        let zone = match object.zone.as_str() {
            "hand" => ZoneKind::Hand,
            "ex" => ZoneKind::Ex,
            "cemetery" => ZoneKind::Cemetery,
            _ => return Ok(false),
        };
        for located in self.internal_printed_abilities(holder)? {
            let ability = &located.node;
            let Some(rule) = card_play_rule(ability)
                .filter(|rule| rule.forbids == forbids && ability.active_zones.contains(&zone))
            else {
                continue;
            };
            if self.internal_card_play_rule_applies(holder, actor, ability, rule.condition)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn internal_card_play_rule_applies(
        &self,
        holder: &str,
        actor: PlayerId,
        ability: &Ability,
        predicate: &Expr<Bool>,
    ) -> Result<bool> {
        if let Expr::Literal(value) = predicate {
            return Ok(value.0);
        }
        // Legality probes must leave allocation clocks and ability instances untouched.
        let mut probe = self.clone();
        let mut execution = probe.new_card_execution(Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller: actor,
        })?;
        let source = probe.internal_card_source(holder, ability.id, actor)?;
        execution.bind_source(&probe, source)?;
        Ok(execution.boolean(&probe, predicate)?.0)
    }
}
