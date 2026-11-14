use super::super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, invalid, seat_name,
};
use crate::game::tokens::prints_for_names;
use crate::ir::{
    DecisionId, Instruction, MoveResult, Node, PlayerId, RuntimeObjectView, TransformOutcome,
    ZoneKind,
};

/// A transformation that banished fewer objects than its templates resumes from these results.
#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TransformResume {
    pub(in crate::game::internal) decision: DecisionId,
    pub(in crate::game::internal) banished: Vec<RuntimeObjectView>,
    pub(in crate::game::internal) moves: Vec<MoveResult>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct CapacityWait {
    pub(super) instruction: Instruction,
    pub(super) prints: Vec<String>,
    pub(super) names: Vec<String>,
    pub(super) choices: Vec<Vec<String>>,
    pub(super) zone: ZoneKind,
    pub(super) actor: PlayerId,
    pub(super) available: usize,
    /// Present when the limit is the transformation's successful count, not zone space.
    pub(super) transform: Option<TransformResume>,
}

impl CapacityWait {
    pub(in crate::game::internal) const fn player_details(&self) -> (PlayerId, ZoneKind, usize) {
        (self.actor, self.zone, self.available)
    }

    pub(in crate::game::internal) fn validate(&self, handle: u64) -> Result<Vec<String>> {
        let chosen = usize::try_from(handle)
            .ok()
            .and_then(|index| self.choices.get(index))
            .ok_or_else(|| invalid("invalid token capacity choice"))?;
        prints_for_names(&self.prints, &self.names, chosen)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Token capacity shares the existing creation transaction and Ward continuation."
)]
impl Game {
    pub(in crate::game::internal::tokens) fn internal_token_capacity_wait(
        &mut self,
        execution: &mut Execution,
        wait: CapacityWait,
    ) -> Result<()> {
        let domain = InputDomain::TokenCapacity {
            choices: wait.choices.clone(),
        };
        let decision = wait
            .transform
            .as_ref()
            .map_or(DecisionId(wait.instruction.id.0), |resume| resume.decision);
        self.internal_wait(
            execution,
            decision,
            wait.actor,
            domain,
            Wait::TokenCapacity(Box::new(wait)),
        )
    }

    pub(in crate::game::internal) fn internal_token_capacity_answer(
        &mut self,
        execution: &mut Execution,
        wait: &CapacityWait,
        prints: &[String],
    ) -> Result<()> {
        let seat = seat_name(wait.actor)?;
        if let Some(resume) = &wait.transform {
            let ids = self.internal_create_prints(prints, seat)?;
            let created = self.commit_token_objects(execution, ZoneKind::Ex, &ids)?;
            execution.receipts.insert(
                wait.instruction.receipt,
                InstructionOutcome::Transform(TransformOutcome {
                    banished: resume.banished.clone(),
                    created,
                    moves: resume.moves.clone(),
                }),
            );
            return Ok(());
        }
        let zone = if wait.zone == ZoneKind::Field {
            "field"
        } else {
            "ex"
        };
        if self.token_capacity(seat, zone)? != wait.available {
            return Err(invalid("token capacity changed during its decision"));
        }
        let ids = self.internal_create_prints(prints, seat)?;
        self.commit_tokens(
            execution,
            &wait.instruction,
            wait.zone,
            ids,
            Node::Instruction {
                instruction: Box::new(wait.instruction.clone()),
            },
        )
    }

    pub(in crate::game::internal::tokens) fn token_capacity(
        &self,
        seat: &str,
        zone: &str,
    ) -> Result<usize> {
        usize::try_from(5_i64.saturating_sub(self.zone_count(seat, zone)).max(0)).map_err(invalid)
    }

    pub(in crate::game::internal) fn internal_create_prints(
        &mut self,
        prints: &[String],
        seat: &str,
    ) -> Result<Vec<String>> {
        prints
            .iter()
            .map(|card| self.new_token_object(card, 0, seat))
            .collect()
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "A partial transformation asks the same template-combination question."
)]
impl Game {
    /// Asks which templates the successful count creates; returns false when no choice remains.
    pub(in crate::game::internal) fn internal_transform_templates(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        requests: &[(String, u64)],
        actor: PlayerId,
        limit: usize,
        resume: TransformResume,
    ) -> Result<Option<Vec<String>>> {
        match self.allocate_token_requests(requests, ZoneKind::Ex, actor, Some(limit))? {
            super::TokenAllocation::Created(ids) => Ok(Some(ids)),
            super::TokenAllocation::Capacity {
                prints,
                names,
                choices,
                available,
            } => {
                self.internal_token_capacity_wait(
                    execution,
                    CapacityWait {
                        instruction: instruction.clone(),
                        prints,
                        names,
                        choices,
                        zone: ZoneKind::Ex,
                        actor,
                        available,
                        transform: Some(resume),
                    },
                )?;
                Ok(None)
            }
        }
    }
}
