//! Only a checked intrinsic evolution projection may enter the shared rule procedure.
use super::{
    Ability, AbilityOrigin, Action, CostSpec, Expr, Game, KeywordId, Located, Node, Result,
    SourceSpan, invalid, unsupported,
};
use crate::game::string;
use crate::ir::evolution_cemetery;
use crate::ir::{
    Bool, Control, Int, PlayerRef, PlayerRelation, Predicate, Resource, ResourceOperation,
    SelectionView, Selector, TextQuantifier, ZoneKind,
};
use crate::ir::{PreconditionKind, PreconditionReason};
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

enum Qualification<'ability> {
    Field(&'ability Expr<Bool>),
    Cemetery { cost: i64, minimum: i64 },
}

fn qualification(ability: &Ability) -> Result<Option<Qualification<'_>>> {
    let CostSpec::Ordered { steps } = &ability
        .play
        .as_ref()
        .ok_or_else(|| invalid("missing evolution play"))?
        .costs
    else {
        return Err(unsupported("evolution costs"));
    };
    let [reveal, pp] = steps.as_slice() else {
        return Err(unsupported("evolution cost projection"));
    };
    if pp.instruction.precondition.is_some() {
        return Err(unsupported("evolution PP precondition"));
    }
    let Some(condition) = &reveal.instruction.precondition else {
        return Ok(None);
    };
    if condition.kind != PreconditionKind::CardTextPrerequisite
        || condition.protects != reveal.instruction.id
        || condition.reason != (PreconditionReason::CardText {})
    {
        return Err(unsupported("evolution qualification guard"));
    }
    if let Some((cost, minimum)) = evolution_cemetery::parameters(&condition.predicate) {
        return Ok(Some(Qualification::Cemetery { cost, minimum }));
    }
    let Expr::Compute(operation) = &condition.predicate else {
        return Err(unsupported("evolution qualification"));
    };
    let Predicate::Exists {
        objects:
            selector @ Selector::Zone {
                players,
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                predicate,
                ..
            },
    } = operation.as_ref()
    else {
        return Err(unsupported("evolution public field qualification"));
    };
    if players.relation != PlayerRelation::SelfPlayer
        || players.relative_to != (PlayerRef::Controller {})
        || players.cardinality.minimum.0 != 0
        || players.cardinality.maximum.is_some()
        || players.text_quantifier != TextQuantifier::Implicit
    {
        return Err(unsupported("evolution qualification controller"));
    }
    super::selection::field_selector(selector)?;
    Ok(Some(Qualification::Field(predicate)))
}

fn usable(game: &Game, holder: &str, ability: &Ability) -> Result<bool> {
    let Some(qualification) = qualification(ability)? else {
        return Ok(true);
    };
    let controller = &game.object(holder)?.controller;
    match qualification {
        Qualification::Field(predicate) => {
            for id in game.zone_ids(controller, "field") {
                if super::selection::matches_candidate(game, &id, predicate)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Qualification::Cemetery { cost, minimum } => {
            let mut count = 0_i64;
            for id in game.zone_ids(controller, "cemetery") {
                if super::super::cost::printed_cost(&game.internal_cost_print(&id)?["cost"])?
                    == cost
                {
                    count = count
                        .checked_add(1)
                        .ok_or_else(|| invalid("cemetery count overflow"))?;
                }
            }
            Ok(count >= minimum)
        }
    }
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
    qualification(&ability.node)?;
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
        self.evolve_with_code(decision, &code, true)
    }
}
