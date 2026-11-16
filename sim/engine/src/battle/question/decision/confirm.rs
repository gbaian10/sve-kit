use super::AnswerError;
use crate::game::player;
use crate::ir::Nat;
use sve_battle_contract::{Answer, question as wire};

#[derive(Clone, Copy)]
pub(in crate::battle::question) enum Binding {
    OptionalCost,
    ResolutionPayment,
    May,
}

pub(super) fn project(
    binding: Binding,
) -> (wire::QuestionUse, wire::QuestionShape, super::Binding) {
    let usage = match binding {
        Binding::OptionalCost => wire::QuestionUse::OptionalCost,
        Binding::ResolutionPayment => wire::QuestionUse::ResolutionPayment,
        Binding::May => wire::QuestionUse::MayEffect,
    };
    (
        usage,
        wire::QuestionShape::Confirm {
            allowed: vec![false, true],
            subject: None,
            origin: None,
        },
        super::Binding::Confirm(binding),
    )
}

impl Binding {
    pub(super) fn answer(self, answer: &Answer) -> Result<player::Answer, AnswerError> {
        let Answer::Confirm { accept } = answer else {
            return Err(AnswerError);
        };
        Ok(match self {
            Self::OptionalCost | Self::ResolutionPayment => player::Answer::OptionalCost(*accept),
            Self::May => player::Answer::Integer(Nat(u64::from(*accept))),
        })
    }
}
