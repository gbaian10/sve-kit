//! Waiting summaries reuse authorized board contexts instead of another seat's question payload.

use super::{
    BattleReferences, Game, Input, ProjectionError, QuestionId, Viewer, build_observation,
};
use crate::battle::InputProjection;
use sve_battle_contract::{
    BattleInput, Seat,
    event::ResultReason,
    question::{PublicDice, PublicWaiting},
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Input projection shares the authoritative player readers."
)]
impl Game {
    /// Bind this seat's complete board, wire input and answer entry to one stable stop.
    ///
    /// The host issues a fresh ID for each question occurrence, authenticates the
    /// viewer and submits decoded commands to this same Game. A full program
    /// closure preflight remains required before interactive use.
    ///
    /// # Errors
    /// Refuses unavailable board facts, unsupported questions, mismatched reference
    /// backgrounds and inexact wire numbers. Projection failures are not engine faults.
    pub fn battle_input(
        &self,
        viewer: Viewer,
        id: &QuestionId,
        references: &BattleReferences,
    ) -> Result<InputProjection, ProjectionError> {
        references.validate_catalog(&self.catalog)?;
        let can_concede =
            self.state.game["ended"] != true && self.state.ir_epoch.checked_add(1).is_some();
        let native = self.input(viewer);
        if let Input::Question(question) = native {
            return self
                .project_battle_question(viewer, id, *question, references)
                .map(|question| InputProjection::question(question, viewer, can_concede));
        }
        let observation = if native == Input::EngineFault {
            // A fault has no rule question; retained native disclosure remains authoritative.
            let board = self
                .observe(viewer)
                .map_err(|_error| ProjectionError::Unavailable)?;
            build_observation(&self.catalog, references, &board, vec![])?
        } else {
            self.battle_observation(viewer, references)?
        };
        let input = match native {
            Input::Waiting(waiting) => BattleInput::Waiting {
                actor: seat(waiting.actor),
                context: Box::new(PublicWaiting {
                    // This summarizes disclosed processing, without claiming a hidden inner frame is current.
                    processing: observation.processing.last().cloned(),
                    dice: waiting
                        .dice
                        .map(|(value, remaining)| -> Result<_, ProjectionError> {
                            Ok(PublicDice {
                                value: value.0.try_into()?,
                                remaining: remaining.0.try_into()?,
                            })
                        })
                        .transpose()?,
                }),
            },
            Input::Finished { winner } => BattleInput::Finished {
                winner: winner.map(seat),
                reason: self.battle_result_reason(winner)?,
            },
            Input::EngineFault => BattleInput::EngineFault { can_concede },
            Input::Question(_) => return Err(ProjectionError::Unavailable),
        };
        Ok(InputProjection::passive(
            observation,
            input,
            viewer,
            can_concede,
        ))
    }

    fn battle_result_reason(
        &self,
        winner: Option<Viewer>,
    ) -> Result<ResultReason, ProjectionError> {
        match self.state.game["by"].as_str() {
            Some("1.2.3") if winner.is_some() => Ok(ResultReason::Concession),
            Some("1.2.4") => Ok(ResultReason::Effect),
            Some("1.2.2") if winner.is_none() => Ok(ResultReason::SimultaneousDefeat),
            Some("11.2.1" | "11.2.2") if winner.is_some() => Ok(ResultReason::Defeat),
            _ => Err(ProjectionError::Unavailable),
        }
    }
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
