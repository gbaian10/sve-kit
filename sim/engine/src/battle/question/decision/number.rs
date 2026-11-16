use super::{AnswerError, Result};
use crate::battle::selection::bounds;
use crate::game::player;
use crate::ir::{Cardinality, Nat};
use core::result;
use sve_battle_contract::Nat as WireNat;
use sve_battle_contract::{Answer, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) enum Binding {
    Integer(wire::Bounds),
    DiceReroll { reroll_allowed: bool },
}

pub(super) fn dice(
    value: Nat,
    remaining: Nat,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let reroll_allowed = remaining.0 > 0;
    let mut allowed = vec![wire::DiceChoice::Keep];
    if reroll_allowed {
        allowed.push(wire::DiceChoice::Reroll);
    }
    Ok((
        wire::QuestionUse::DiceReroll,
        wire::QuestionShape::DiceReroll {
            value: WireNat::try_from(value.0)?,
            remaining: WireNat::try_from(remaining.0)?,
            allowed,
        },
        super::Binding::Number(Binding::DiceReroll { reroll_allowed }),
    ))
}

pub(super) fn integer(
    domain: Cardinality,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let bounds = bounds(domain)?;
    Ok((
        wire::QuestionUse::DeclareX,
        wire::QuestionShape::Integer {
            bounds,
            // Numeric bounds carry neither a unit nor an exact text coordinate.
            unit: None,
            source: None,
        },
        super::Binding::Number(Binding::Integer(bounds)),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        match self {
            Self::Integer(bounds) => {
                let Answer::Integer { value } = answer else {
                    return Err(AnswerError);
                };
                if value.get() < bounds.min.get()
                    || bounds.max.is_some_and(|max| value.get() > max.get())
                {
                    return Err(AnswerError);
                }
                Ok(player::Answer::Integer(Nat(value.get())))
            }
            Self::DiceReroll { reroll_allowed } => {
                let Answer::DiceReroll { choice } = answer else {
                    return Err(AnswerError);
                };
                match choice {
                    wire::DiceChoice::Keep => Ok(player::Answer::Integer(Nat(0))),
                    wire::DiceChoice::Reroll if *reroll_allowed => {
                        Ok(player::Answer::Integer(Nat(1)))
                    }
                    wire::DiceChoice::Reroll => Err(AnswerError),
                }
            }
        }
    }
}
