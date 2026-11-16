use super::*;
use crate::ir::{Nat, QuickWindow};

#[cfg(feature = "runner")]
mod joint;
#[cfg(feature = "runner")]
mod native;
mod number;
#[cfg(feature = "runner")]
mod select;

#[test]
fn confirms_keep_distinct_uses_and_native_answer_channels() {
    for (domain, usage, no, yes) in [
        (
            DecisionShape::OptionalCost,
            wire::QuestionUse::OptionalCost,
            player::Answer::OptionalCost(false),
            player::Answer::OptionalCost(true),
        ),
        (
            DecisionShape::ResolutionPayment,
            wire::QuestionUse::ResolutionPayment,
            player::Answer::OptionalCost(false),
            player::Answer::OptionalCost(true),
        ),
        (
            DecisionShape::May,
            wire::QuestionUse::MayEffect,
            player::Answer::Integer(Nat(0)),
            player::Answer::Integer(Nat(1)),
        ),
    ] {
        let (actual_use, shape, binding) = project(&domain, None).unwrap();
        assert_eq!(actual_use, usage);
        assert_eq!(
            shape,
            wire::QuestionShape::Confirm {
                allowed: vec![false, true],
                subject: None,
                origin: None,
            }
        );
        assert_eq!(binding.decode(&Answer::Confirm { accept: false }), Ok(no));
        assert_eq!(binding.decode(&Answer::Confirm { accept: true }), Ok(yes));
        assert_eq!(
            binding.decode(&Answer::Integer {
                value: sve_battle_contract::Nat::from(1_u32),
            }),
            Err(AnswerError)
        );
    }
}

#[test]
fn unsupported_decisions_do_not_fall_back_to_confirm() {
    for domain in [
        DecisionShape::BeforeDraw,
        DecisionShape::SearchReplacement,
        DecisionShape::ExchangeRoutes(vec![Nat(0), Nat(2)]),
        DecisionShape::Quick {
            window: QuickWindow::EndPhase,
            options: vec![],
        },
        DecisionShape::Pending(vec![]),
    ] {
        assert!(matches!(
            project(&domain, None),
            Err(ProjectionError::Unavailable)
        ));
    }
}
