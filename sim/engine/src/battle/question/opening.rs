use super::common::{card_candidates, hand_candidates, option, plain_selection, seat};
use super::{AnswerError, Binding, NativeHandle, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, OpeningAnswer, Viewer};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{
    Nat, OptionId, QuestionId, Zone, observation::BattleObservation, question as wire,
};

pub(super) fn first_player(id: &QuestionId) -> (wire::QuestionUse, wire::QuestionShape, Binding) {
    let choices = [Viewer::P1, Viewer::P2]
        .into_iter()
        .enumerate()
        .map(|(index, viewer)| (option(id, index), viewer))
        .collect::<BTreeMap<_, _>>();
    let options = choices
        .iter()
        .map(|(choice, viewer)| wire::ChoiceOption {
            id: choice.clone(),
            value: wire::ChoiceValue::Seat {
                player: seat(*viewer),
            },
            cost: None,
        })
        .collect();
    (
        wire::QuestionUse::FirstPlayer,
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
        Binding::FirstPlayer(choices),
    )
}

pub(super) fn mulligan(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    let (candidates, coordinates) = hand_candidates(id, actor, cards, observation)?;
    Ok((
        wire::QuestionUse::Mulligan,
        wire::QuestionShape::Mulligan { hand: candidates },
        Binding::Mulligan(coordinates),
    ))
}

pub(super) fn start_amulet(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native opening skips this question without eligible cards; an empty domain is inconsistent.
    if cards.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let deck = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .deck;
    let (candidates, coordinates) =
        card_candidates(id, actor, cards, observation, Zone::Deck, deck)?;
    Ok((
        wire::QuestionUse::StartAmulet,
        plain_selection(
            candidates,
            wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
        ),
        Binding::StartAmulet(coordinates),
    ))
}

pub(super) fn mulligan_answer(
    cards: &BTreeMap<OptionId, NativeHandle>,
    redraw: bool,
    bottom_order: &[OptionId],
) -> result::Result<OpeningAnswer, AnswerError> {
    if !redraw {
        return if bottom_order.is_empty() {
            Ok(OpeningAnswer::Keep)
        } else {
            Err(AnswerError)
        };
    }
    if bottom_order.len() != cards.len() {
        return Err(AnswerError);
    }
    let mut seen = BTreeSet::new();
    let order = bottom_order
        .iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err(AnswerError);
            }
            cards.get(id).cloned().ok_or(AnswerError)
        })
        .collect::<result::Result<Vec<_>, _>>()?;
    Ok(OpeningAnswer::Mulligan(order))
}
