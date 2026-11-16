use super::candidates::Candidates;
use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, CardOption};
use crate::ir::TriggerIcon;
use core::result;
use sve_battle_contract::{Answer, Nat, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    candidates: Candidates,
    target_required: bool,
}

pub(super) fn project(
    icon: TriggerIcon,
    targets: &[CardOption],
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if context.purpose != player::QuestionPurpose::RuleChoice {
        return Err(ProjectionError::Unavailable);
    }
    let (icon, designates) = match icon {
        TriggerIcon::Critical => (wire::TriggerIcon::Critical, true),
        TriggerIcon::Stand => (wire::TriggerIcon::Stand, true),
        TriggerIcon::Draw => (wire::TriggerIcon::Draw, false),
        TriggerIcon::Heal => (wire::TriggerIcon::Heal, false),
        TriggerIcon::Front => return Err(ProjectionError::Unavailable),
    };
    if !designates && !targets.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let candidates = Candidates::drive(targets, context)?;
    // Native Drive skips an impossible designation rather than disabling acceptance.
    let target_required = designates && !targets.is_empty();
    let count = Nat::from(u32::from(target_required));
    Ok((
        wire::QuestionUse::DriveTrigger,
        wire::QuestionShape::DriveTrigger {
            icon,
            allowed: vec![false, true],
            targets: candidates.values.clone(),
            target_bounds: wire::Bounds {
                min: count,
                max: Some(count),
            },
        },
        super::Binding::Drive(Binding {
            candidates,
            target_required,
        }),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::DriveTrigger { accept, target } = answer else {
            return Err(AnswerError);
        };
        if target.is_some() != (*accept && self.target_required) {
            return Err(AnswerError);
        }
        let handles = self
            .candidates
            .lookup(&target.iter().cloned().collect::<Vec<_>>())?;
        Ok(player::Answer::DriveTrigger {
            accept: *accept,
            target: handles.first().copied(),
        })
    }
}
