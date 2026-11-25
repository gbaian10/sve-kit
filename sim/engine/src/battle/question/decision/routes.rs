use super::{AnswerError, Context, Result};
use crate::battle::question::common::{option, seat};
use crate::battle::{ProjectionError, costs, selection};
use crate::game::player::observation::ObservedZone;
use crate::game::player::observation::QuoteBasis;
use crate::game::player::{
    self, DecisionShape, ExchangeRouteChoice, SearchReplacementQuestion, SelectionDestination,
};
use crate::ir::SearchReplacementAnswer;
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Answer, Nat, OptionId, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, player::Answer>,
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if let DecisionShape::SearchReplacement(destinations) = domain {
        search(*destinations, context)
    } else if let DecisionShape::ExchangeRoutes(choices) = domain {
        exchange(choices, context)
    } else {
        Err(ProjectionError::Unavailable)
    }
}

fn search(
    domain: SearchReplacementQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let routes = [
        (
            wire::ReplacementRoute::Original,
            SearchReplacementAnswer::Original,
            domain.original,
        ),
        (
            wire::ReplacementRoute::Replacement,
            SearchReplacementAnswer::Replacement,
            domain.replacement,
        ),
    ];
    let mut coordinates = BTreeMap::new();
    let mut options = vec![];
    for (index, (route, native, destination)) in routes.into_iter().enumerate() {
        let destination = destination.ok_or(ProjectionError::Unavailable)?;
        let id = option(context.id, index);
        coordinates.insert(id.clone(), player::Answer::SearchReplacement(native));
        options.push(wire::ChoiceOption {
            id,
            value: wire::ChoiceValue::Destination {
                route,
                destination: wire_destination(destination)?,
            },
            cost: None,
        });
    }
    Ok(finish(
        wire::QuestionUse::SearchReplacement,
        options,
        coordinates,
    ))
}

fn exchange(
    choices: &[ExchangeRouteChoice],
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if choices.is_empty() || context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let mut seen = BTreeSet::new();
    let mut coordinates = BTreeMap::new();
    let mut options = vec![];
    for (index, choice) in choices.iter().enumerate() {
        if !seen.insert(choice.index.0) || choice.cost.basis != QuoteBasis::BeforePayment {
            return Err(ProjectionError::Unavailable);
        }
        let id = option(context.id, index);
        coordinates.insert(id.clone(), player::Answer::Integer(choice.index));
        options.push(wire::ChoiceOption {
            id,
            value: wire::ChoiceValue::Route {
                route: Nat::try_from(choice.index.0)?,
            },
            cost: Some(costs::summary(&choice.cost)?),
        });
    }
    Ok(finish(
        wire::QuestionUse::ExchangeRoute,
        options,
        coordinates,
    ))
}

fn finish(
    usage: wire::QuestionUse,
    options: Vec<wire::ChoiceOption>,
    coordinates: BTreeMap<OptionId, player::Answer>,
) -> (wire::QuestionUse, wire::QuestionShape, super::Binding) {
    (
        usage,
        wire::QuestionShape::Choose {
            options,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: None,
        },
        super::Binding::Routes(Binding { coordinates }),
    )
}

const fn wire_destination(destination: SelectionDestination) -> Result<wire::Destination> {
    let zone = match destination.zone {
        ObservedZone::Hand | ObservedZone::Field | ObservedZone::Ex => {
            selection::zone(destination.zone)
        }
        ObservedZone::Deck
        | ObservedZone::EvolveDeck
        | ObservedZone::Cemetery
        | ObservedZone::Banish
        | ObservedZone::Resolution
        | ObservedZone::Evolution
        | ObservedZone::Race
        | ObservedZone::Drive
        | ObservedZone::Trigger
        | ObservedZone::Equipment
        | ObservedZone::LimitBreak => return Err(ProjectionError::Unavailable),
    };
    Ok(wire::Destination {
        player: seat(destination.player),
        zone,
        position: None,
    })
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Choose { chosen } = answer else {
            return Err(AnswerError);
        };
        let [choice] = chosen.as_slice() else {
            return Err(AnswerError);
        };
        self.coordinates.get(choice).cloned().ok_or(AnswerError)
    }
}
