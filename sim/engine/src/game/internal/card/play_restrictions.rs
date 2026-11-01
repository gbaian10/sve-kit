use super::{Ability, Context, Game, Generation, Located, PlayerId, Result, unsupported};
use crate::ir::play_restrictions::card_restriction;
use crate::ir::{Expr, ZoneKind};

pub(super) fn check(located: &Located<Ability>, ordinal: usize) -> Result<bool> {
    let Some(predicate) = card_restriction(&located.node) else {
        return Ok(false);
    };
    if ordinal != 0 {
        return Err(unsupported("play restriction requires base face"));
    }
    super::super::ordering::check_condition(predicate, &located.source)?;
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
        let object = self.object(holder)?;
        for located in self.internal_printed_abilities(holder)? {
            let ability = &located.node;
            let in_zone = match object.zone.as_str() {
                "hand" => ability.active_zones.contains(&ZoneKind::Hand),
                "ex" => ability.active_zones.contains(&ZoneKind::Ex),
                _ => false,
            };
            let Some(predicate) = card_restriction(ability).filter(|_| in_zone) else {
                continue;
            };
            if let Expr::Literal(value) = predicate {
                if value.0 {
                    return Ok(true);
                }
                continue;
            }
            // Legality probes must leave allocation clocks and ability instances untouched.
            let mut probe = self.clone();
            let mut execution = probe.new_card_execution(Context {
                source: holder.into(),
                generation: Generation(object.generation),
                controller: actor,
            })?;
            let source = probe.internal_card_source(holder, ability.id, actor)?;
            execution.bind_source(&probe, source)?;
            if execution.boolean(&probe, predicate)?.0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
