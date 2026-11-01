//! Only a checked intrinsic evolution projection may enter the shared rule procedure.
use super::{
    Ability, AbilityOrigin, Action, CostSpec, Expr, Game, Generation, KeywordId, Located, Node,
    Result, SourceSpan, invalid, unsupported,
};
use crate::game::string;
use crate::ir::{Bool, Control, Int, PlayerRef, Resource, ResourceOperation};
use serde_json::{Value, json};

pub(super) const fn intrinsic(ability: &Ability) -> bool {
    matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Evolution,
            ..
        }
    )
}

pub(super) fn cost(ability: &Ability) -> Result<i64> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing evolution play"))?;
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("evolution costs"));
    };
    steps
        .iter()
        .skip(1)
        .try_fold(0_i64, |sum, step| match &step.instruction.action {
            Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(value)),
            } => sum
                .checked_add(*value)
                .ok_or_else(|| invalid("evolution cost overflow")),
            _ => Ok(sum),
        })
}

fn qualification(ability: &Ability) -> Result<Option<&Expr<Bool>>> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing evolution play"))?;
    super::qualification(&play.costs)
}

fn usable(game: &Game, holder: &str, ability: &Ability) -> Result<bool> {
    let Some(predicate) = qualification(ability)? else {
        return Ok(true);
    };
    let object = game.object(holder)?;
    let controller = super::player_id(&object.controller)?;
    let context = super::Context {
        source: holder.into(),
        generation: Generation(object.generation),
        controller,
    };
    // A legality probe must not allocate clocks or ability instances in the real game.
    let mut probe = game.clone();
    let mut execution = probe.new_card_execution(context)?;
    let source = probe.internal_card_source(holder, ability.id, controller)?;
    execution.bind_source(&probe, source)?;
    Ok(execution.boolean(&probe, predicate)?.0)
}

fn span(game: &Game, source: &SourceSpan, expected: &SourceSpan) -> Result<()> {
    if source.owner != expected.owner
        || source.field != expected.field
        || source.fragment != expected.fragment
    {
        return Err(invalid("evolution source evidence mapping mismatch"));
    }
    game.check_card_source(
        source,
        &expected.card_no,
        &expected.face,
        u64::from(expected.line),
    )
}

pub(super) fn check(game: &Game, ability: &Located<Ability>) -> Result<()> {
    cost(&ability.node)?;
    if ability
        .node
        .play
        .as_ref()
        .is_some_and(|play| play.parameters.len() > 2)
    {
        return Err(unsupported("one evolution material parameter"));
    }
    let mut candidate = ability.node.clone();
    let candidate_play = candidate
        .play
        .as_mut()
        .ok_or_else(|| invalid("missing evolution play"))?;
    let CostSpec::Ordered { steps: payments } = &mut candidate_play.costs else {
        return Err(unsupported("evolution costs"));
    };
    payments.remove(0);
    candidate_play.parameters.remove(0);
    super::check_costs(&candidate, &ability.source)?;
    if let Some(predicate) = qualification(&ability.node)? {
        super::super::ordering::check_condition(predicate, &ability.source)?;
    }
    let body = ability
        .node
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing evolution body"))?;
    let [projection] = body.steps.as_slice() else {
        return Err(unsupported("evolution projection"));
    };
    span(game, &projection.source, &ability.source)?;
    let Node::Control {
        control: Control::EachObject {
            body: projected, ..
        },
    } = &projection.node
    else {
        return Err(unsupported("evolution body"));
    };
    let [step] = projected.steps.as_slice() else {
        return Err(unsupported("evolution instruction"));
    };
    span(game, &step.source, &ability.source)?;
    let Node::Instruction { instruction } = &step.node else {
        return Err(unsupported("evolution instruction"));
    };
    if instruction.source != ability.node.source {
        return Err(invalid("foreign evolution instruction source"));
    }
    let play = ability
        .node
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing evolution play"))?;
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("evolution costs"));
    };
    if steps
        .iter()
        .any(|payment| payment.instruction.source != ability.node.source)
    {
        return Err(invalid("foreign evolution cost source"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The intrinsic proof delegates to the existing validated rule procedure."
)]
impl Game {
    pub(in crate::game) fn internal_evolution_payment(
        &mut self,
        ability: &Ability,
        decision: &Value,
    ) -> Result<bool> {
        let mut play = ability
            .play
            .clone()
            .ok_or_else(|| invalid("evolution play"))?;
        play.parameters.remove(0);
        let CostSpec::Ordered { steps } = &mut play.costs else {
            return Err(unsupported("evolution costs"));
        };
        steps.remove(0);
        steps.retain(|step| {
            !matches!(
                step.instruction.action,
                Action::Resource {
                    resource: Resource::PpCurrent,
                    operation: ResourceOperation::Pay,
                    ..
                }
            )
        });
        if steps.is_empty() {
            return Ok(true);
        }
        let mut probe = self.clone();
        let holder = string(&decision["source"]);
        let object = self.object(holder)?;
        let controller = super::player_id(&object.controller)?;
        let mut execution = probe.new_card_execution(super::Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller,
        })?;
        let origin = probe.internal_card_source(holder, ability.id, controller)?;
        execution.bind_source(&probe, origin)?;
        probe.card_runtime.active_source = Some(origin);
        let attempt = (|| {
            for parameter in &play.parameters {
                probe.internal_material_parameter(
                    &mut execution,
                    parameter,
                    super::material::posture_parameter(&play.costs, parameter),
                )?;
                let Some((_, super::super::Wait::MaterialParameter(wait))) =
                    execution.waiting.take()
                else {
                    return Err(invalid("evolution material input"));
                };
                let ids = inline_ids(&decision["costs"]["materials"])?;
                if ids.iter().any(|id| id == holder) {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "evolution cost moved its source".into(),
                    ));
                }
                wait.inline_bind(&mut execution, &ids)?;
            }
            if probe.internal_hand_cost_materials(&mut execution, &play.costs)? {
                let Some((_, super::super::Wait::CostDiscard(wait))) = execution.waiting.take()
                else {
                    return Err(invalid("evolution hand input"));
                };
                let ids = inline_ids(&decision["costs"]["discard"])?;
                let members = wait.inline_members(&probe, &execution, &ids)?;
                execution.cost_discards.insert(wait.receipt(), members);
            }
            probe.internal_card_payment(&mut execution, &play.costs, false)
        })();
        match attempt {
            Ok(()) => {
                probe.card_runtime.active_source = self.card_runtime.active_source;
                *self = probe;
                Ok(true)
            }
            Err(crate::EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(in crate::game) fn internal_evolve_action(&mut self, decision: &Value) -> Result<bool> {
        let holder = string(&decision["source"]);
        let object = self.object(holder)?;
        if object.state["evolved_with"].is_string()
            || object.state["face"] != 0_i64
            || decision.get("ability").is_some()
        {
            return Ok(false);
        }
        if decision
            .get("face")
            .is_some_and(|face| face.as_u64() != Some(0))
        {
            return Ok(false);
        }
        for component in ["pp", "ep", "sep"] {
            if decision["pay"]
                .get(component)
                .is_some_and(|value| value.as_i64().is_none())
            {
                return Ok(false);
            }
        }
        self.object(string(&decision["evolve_card"]))?
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("evolution resource generation exhausted"))?;
        let entries = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .filter(|a| intrinsic(&a.node))
            .filter(|a| {
                decision
                    .get("definition")
                    .is_none_or(|id| id.as_u64() == Some(a.node.id.0))
            })
            .collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            return Ok(false);
        };
        if !usable(self, holder, &entry.node)? {
            return Ok(false);
        }
        let amount = cost(&entry.node)?;
        // The fixed well-formed projection proves exactly this name-bound, ordinary rule entry.
        let code = json!({"kind":"evolve","costs":[{"op":"pp","amount":amount}],"body":{"op":"evolve","subjects":"self"}});
        self.evolve_with_code(decision, &code, Some(&entry.node))
    }
}

fn inline_ids(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| {
            crate::EngineFailure::IllegalPlay("missing evolution cost materials".into())
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("cost material ID"))
        })
        .collect()
}
