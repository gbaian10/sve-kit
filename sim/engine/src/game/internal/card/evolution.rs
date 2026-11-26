//! Intrinsic evolution and its field permission share the existing rule procedure.
use super::{
    Ability, AbilityId, AbilityOrigin, Action, Context, CostSpec, Execution, Expr, Game,
    Generation, KeywordId, Located, Node, Result, SourceSpan, invalid, unsupported,
};
use crate::game::string;
use crate::ir::evolution_destination::Destination;
use crate::ir::evolution_limits;
use crate::ir::{
    Bool, ChoiceDomain, Control, Int, PlaySpec, PlayerId, PlayerRef, Resource, ResourceOperation,
    SelectionGroup, Selector,
};
use serde_json::{Value, json};

mod declaration;
pub(in crate::game) use declaration::Declaration as EvolutionDeclaration;

fn rule_code(ability: &Ability) -> Result<Value> {
    let amount = cost(ability)?;
    let mut code = json!({"kind":"evolve","costs":[{"op":"pp","amount":amount}]});
    let mut body = destination_spec(&destination(ability)?);
    body["op"] = json!("evolve");
    body["subjects"] = json!("self");
    code["body"] = body;
    Ok(code)
}

pub(super) fn destination_spec(destination: &Destination) -> Value {
    match destination {
        Destination::SameName => json!({}),
        Destination::Names(names) => {
            json!({"names": names.iter().map(|name| &name.0).collect::<Vec<_>>()})
        }
        Destination::Contains(name) => json!({"name_contains": name.0}),
    }
}

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

fn non_pp_play(ability: &Ability) -> Result<PlaySpec> {
    let mut play = ability
        .play
        .clone()
        .ok_or_else(|| invalid("missing evolution play"))?;
    let CostSpec::Ordered { steps } = &mut play.costs else {
        return Err(unsupported("evolution costs"));
    };
    if play.parameters.is_empty() || steps.is_empty() {
        return Err(invalid("missing evolution form parameter or reveal cost"));
    }
    // The rule procedure handles the form reveal and the adjusted PP/EP payment.
    play.parameters.remove(0);
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
    Ok(play)
}

fn qualification(ability: &Ability) -> Result<Option<&Expr<Bool>>> {
    let play = ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing evolution play"))?;
    super::qualification(&play.costs)
}

fn usable(game: &Game, holder: &str, ability: &Ability) -> Result<bool> {
    if game.internal_ability_forbidden(holder, ability)? {
        return Ok(false);
    }
    let Some(predicate) = qualification(ability)? else {
        return Ok(true);
    };
    let controller = super::player_id(&game.object(holder)?.controller)?;
    // A legality probe must not allocate clocks or ability instances in the real game.
    let mut probe = game.clone();
    let execution = probe.evolution_selection(holder, ability.id, controller)?;
    Ok(execution.boolean(&probe, predicate)?.0)
}

fn span(game: &Game, source: &SourceSpan, expected: &SourceSpan) -> Result<()> {
    if source.owner != expected.owner
        || source.field != expected.field
        || source.fragment != expected.fragment
    {
        return Err(invalid("evolution source evidence mapping mismatch"));
    }
    game.check_nested_card_source(source, expected)
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

impl Game {
    fn evolution_selection(
        &mut self,
        holder: &str,
        definition: AbilityId,
        controller: PlayerId,
    ) -> Result<Execution> {
        let mut execution = self.new_card_execution(Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller,
        })?;
        let origin = self.internal_card_source(holder, definition, controller)?;
        execution.bind_source(self, origin)?;
        Ok(execution)
    }

    pub(in crate::game) fn internal_evolution_payment(
        &mut self,
        ability: &Ability,
        decision: &Value,
    ) -> Result<bool> {
        if self.internal_ability_forbidden(string(&decision["source"]), ability)? {
            return Ok(false);
        }
        let play = non_pp_play(ability)?;
        if matches!(&play.costs, CostSpec::Ordered { steps } if steps.is_empty()) {
            return Ok(true);
        }
        let mut probe = self.clone();
        let holder = string(&decision["source"]);
        let controller = super::player_id(&self.object(holder)?.controller)?;
        let mut execution = probe.evolution_selection(holder, ability.id, controller)?;
        probe.card_runtime.active_source = execution.ability;
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
                wait.record(&mut execution, members);
            }
            probe.internal_card_payment(&mut execution, &play.costs, false, None)
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
        self.run_internal_evolution(decision, false)
    }

    fn run_internal_evolution(&mut self, decision: &Value, probe: bool) -> Result<bool> {
        let holder = string(&decision["source"]);
        let object = self.object(holder)?;
        if object.state["evolved_with"].is_string()
            || object.state["face"] != 0_i64
            || decision.get("ability").is_some()
        {
            return Ok(false);
        }
        let face = match decision.get("face") {
            None => 0,
            Some(value) => {
                let Some(face) = value.as_u64().and_then(|value| usize::try_from(value).ok())
                else {
                    return Ok(false);
                };
                face
            }
        };
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
        let code = rule_code(&entry.node)?;
        let resource = self.object(string(&decision["evolve_card"]))?;
        if self.catalog.face(&resource.card, face).is_err() {
            return Ok(false);
        }
        self.evolve_with_code(decision, &code, Some(&entry.node), probe)
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

fn destination(ability: &Ability) -> Result<Destination> {
    let parameter = ability
        .play
        .as_ref()
        .and_then(|play| play.parameters.first())
        .ok_or_else(|| invalid("missing evolution face parameter"))?;
    let ChoiceDomain::Objects { constraint, .. } = &parameter.domain else {
        return Err(unsupported("evolution face domain"));
    };
    let Some(SelectionGroup {
        selector: Selector::Zone {
            predicate, binding, ..
        },
        ..
    }) = constraint.groups.first()
    else {
        return Err(unsupported("evolution candidate domain"));
    };
    Destination::from_predicate(predicate, *binding)
        .ok_or_else(|| unsupported("evolution destination predicate"))
}

pub(in crate::game::internal) fn resource_association(
    game: &Game,
    base: &str,
    form: &str,
) -> Result<bool> {
    let object = game.object(base)?;
    let form_object = game.object(form)?;
    let face = usize::try_from(super::int(&form_object.state["face"])).map_err(invalid)?;
    let base_name = game.catalog.rules_label(&object.card, 0)?;
    let form_name = game.catalog.rules_label(&form_object.card, face)?;
    let is_evolve_follower =
        game.catalog.face(&form_object.card, face)?["card_type"] == "フォロワー・エボルヴ";
    if base_name == form_name {
        return Ok(true);
    }
    let Some(program) = game.card_runtime.definitions.cards.get(&object.card) else {
        return Ok(false);
    };
    let destinations = program
        .card()
        .abilities
        .iter()
        .filter(|entry| intrinsic(&entry.node))
        .map(|entry| destination(&entry.node))
        .collect::<Result<Vec<_>>>()?;
    if destinations
        .iter()
        .any(|dest| dest.matches(&base_name, &form_name, is_evolve_follower))
    {
        return Ok(true);
    }
    // The association survives ability loss after the evolution choice was made.
    let base_names = game.printed_card_names(&object.card, 0, "field")?;
    let form_names = game.printed_card_names(&form_object.card, face, "evolve_deck")?;
    Ok(form_names.iter().any(|candidate_form_name| {
        base_names.iter().any(|candidate_base_name| {
            candidate_base_name == candidate_form_name
                || destinations.iter().any(|dest| {
                    dest.matches(candidate_base_name, candidate_form_name, is_evolve_follower)
                })
        })
    }))
}

impl Game {
    pub(in crate::game) fn internal_unlimited_evolution(&self, seat: &str) -> Result<bool> {
        for provider in self.zone_ids(seat, "field") {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            for ability in self.internal_printed_abilities(&provider)? {
                if evolution_limits::is_static(&ability.node) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
