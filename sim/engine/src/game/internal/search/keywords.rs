use super::{Game, KeywordId, Result};
use crate::ir::{Ability, AbilityOrigin, Control, CostSpec, Node, Program, RepeatBody};

pub(super) fn matches(game: &Game, id: &str, keyword: KeywordId) -> Result<bool> {
    if game.object(id)?.zone == "field" {
        return Ok(game
            .internal_effective_abilities(id)?
            .iter()
            .any(|ability| has_keyword(&ability.node, keyword)));
    }
    Ok(game
        .internal_characteristic_abilities(id)?
        .iter()
        .any(|ability| has_keyword(&ability.node, keyword)))
}

fn has_keyword(ability: &Ability, keyword: KeywordId) -> bool {
    if keyword == KeywordId::Lastword {
        return matches!(
            ability.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Lastword,
                ..
            }
        );
    }
    if keyword != KeywordId::EarthRite {
        return false;
    }
    ability
        .play
        .as_ref()
        .is_some_and(|play| has_cost(&play.costs))
        || ability.body.as_ref().is_some_and(has_program)
}

fn has_cost(cost: &CostSpec) -> bool {
    match cost {
        CostSpec::Ordered { steps } => steps
            .iter()
            .any(|step| step.keyword == Some(KeywordId::EarthRite)),
        CostSpec::Alternative { alternatives } => alternatives.iter().any(has_cost),
    }
}

fn has_program(program: &Program) -> bool {
    program.steps.iter().any(|step| {
        let Node::Control { control } = &step.node else {
            return false;
        };
        match control {
            Control::Pay { costs, on_paid, .. } => has_cost(costs) || has_program(on_paid),
            Control::Mode { modes, .. } => modes
                .iter()
                .any(|mode| has_cost(&mode.costs) || has_program(&mode.body)),
            Control::Sequence { body }
            | Control::May { body, .. }
            | Control::EachObject { body, .. }
            | Control::Repeat {
                body: RepeatBody::Program { body },
                ..
            } => has_program(body),
            Control::If {
                then_body,
                else_body,
                ..
            } => has_program(then_body) || else_body.as_ref().is_some_and(has_program),
            Control::Match {
                leader_body,
                follower_body,
                ..
            } => has_program(leader_body) || has_program(follower_body),
            Control::EachPlayer { stages, .. } => {
                stages.iter().any(|stage| has_program(&stage.body))
            }
            Control::Repeat {
                body: RepeatBody::BodyReference { .. },
                ..
            }
            | Control::Simultaneous { .. } => false,
        }
    })
}
