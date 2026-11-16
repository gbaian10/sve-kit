use super::common::{card_candidates, hand_candidates, plain_selection, seat};
use super::{Binding, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, Viewer};
use sve_battle_contract::{
    Nat, QuestionId, Zone, observation::BattleObservation, question as wire,
};

pub(super) fn hand_limit(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    count: u64,
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native closing skips a hand within its limit; zero or counts beyond the hand are inconsistent.
    let required = usize::try_from(count)
        .ok()
        .filter(|required| *required > 0 && *required <= cards.len())
        .ok_or(ProjectionError::Unavailable)?;
    let count = Nat::try_from(count)?;
    let (candidates, coordinates) = hand_candidates(id, actor, cards, observation)?;
    Ok((
        wire::QuestionUse::HandLimitDiscard,
        plain_selection(
            candidates,
            wire::Bounds {
                min: count,
                max: Some(count),
            },
        ),
        Binding::HandLimit {
            cards: coordinates,
            count: required,
        },
    ))
}

pub(super) fn end_guard(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native closing skips this question without standing guards; an empty domain is inconsistent.
    if cards.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let field = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .field;
    let (candidates, coordinates) =
        card_candidates(id, actor, cards, observation, Zone::Field, field)?;
    Ok((
        wire::QuestionUse::EndGuard,
        plain_selection(
            candidates,
            wire::Bounds {
                min: Nat::from(0_u32),
                max: Some(Nat::try_from(cards.len())?),
            },
        ),
        Binding::EndGuard(coordinates),
    ))
}
