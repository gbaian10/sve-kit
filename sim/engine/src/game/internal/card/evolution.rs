//! Only a checked intrinsic evolution projection may enter the shared rule procedure.
use super::{
    Ability, AbilityOrigin, Action, CostSpec, Expr, Game, KeywordId, Located, Node, Result,
    SourceSpan, check_source, invalid, unsupported,
};
use crate::game::string;
use crate::ir::{Control, Int, PlayerRef, Resource, ResourceOperation};
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
    let [_, pp] = steps.as_slice() else {
        return Err(unsupported("evolution cost projection"));
    };
    let Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(value)),
    } = &pp.instruction.action
    else {
        return Err(unsupported("evolution PP projection"));
    };
    Ok(*value)
}

fn span(source: &SourceSpan, expected: &SourceSpan) -> Result<()> {
    if source.owner != expected.owner
        || source.field != expected.field
        || source.fragment != expected.fragment
    {
        return Err(invalid("evolution source evidence mapping mismatch"));
    }
    check_source(
        source,
        &expected.card_no,
        &expected.face,
        u64::from(expected.line),
    )
}

pub(super) fn check(ability: &Located<Ability>) -> Result<()> {
    cost(&ability.node)?;
    let body = ability
        .node
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing evolution body"))?;
    let [projection] = body.steps.as_slice() else {
        return Err(unsupported("evolution projection"));
    };
    span(&projection.source, &ability.source)?;
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
    span(&step.source, &ability.source)?;
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
        let amount = cost(&entry.node)?;
        // The fixed well-formed projection proves exactly this name-bound, ordinary rule entry.
        let code = json!({"kind":"evolve","costs":[{"op":"pp","amount":amount}],"body":{"op":"evolve","subjects":"self"}});
        self.evolve_with_code(decision, &code, true)
    }
}
