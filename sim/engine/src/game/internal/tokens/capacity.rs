use super::super::{Execution, Game, InputDomain, Result, Wait, invalid, seat_name};
use crate::game::tokens::prints_for_names;
use crate::ir::{DecisionId, Instruction, Node, PlayerId, ZoneKind, ZoneRef};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct CapacityWait {
    pub(super) instruction: Instruction,
    pub(super) prints: Vec<String>,
    pub(super) names: Vec<String>,
    pub(super) choices: Vec<Vec<String>>,
    pub(super) zone: ZoneRef,
    pub(super) actor: PlayerId,
    pub(super) available: usize,
}

impl CapacityWait {
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
        self.internal_wait(
            execution,
            DecisionId(wait.instruction.id.0),
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
        let zone = if wait.zone.kind == ZoneKind::Field {
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
            wait.zone.kind,
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

    pub(in crate::game::internal::tokens) fn internal_create_prints(
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
