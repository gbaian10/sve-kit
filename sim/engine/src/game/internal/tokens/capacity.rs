use super::super::{Execution, Game, InputDomain, Result, Wait, invalid, seat_name};
use crate::game::player::observation::CardFace;
use crate::game::tokens::prints_for_names;
use crate::ir::{DecisionId, Instruction, Node, PlayerId, ZoneKind};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct CapacityWait {
    pub(super) instruction: Instruction,
    pub(super) prints: Vec<String>,
    pub(super) names: Vec<String>,
    pub(super) choices: Vec<Vec<String>>,
    pub(super) zone: ZoneKind,
    pub(super) actor: PlayerId,
    pub(super) available: usize,
}

impl CapacityWait {
    pub(in crate::game::internal) fn player_faces(
        &self,
        game: &Game,
        choices: &[Vec<String>],
    ) -> Result<Vec<Vec<CardFace>>> {
        if choices != self.choices {
            return Err(invalid("token choices differ from their frozen wait"));
        }
        game.internal_token_choice_faces(choices.len(), |handle| self.validate(handle))
    }
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
    pub(in crate::game::internal) fn internal_token_choice_faces<F>(
        &self,
        count: usize,
        mut resolve: F,
    ) -> Result<Vec<Vec<CardFace>>>
    where
        F: FnMut(u64) -> Result<Vec<String>>,
    {
        (0..count)
            .map(|index| {
                resolve(u64::try_from(index).map_err(invalid)?)?
                    .iter()
                    .map(|card| self.internal_creation_face(card))
                    .collect()
            })
            .collect()
    }
    pub(in crate::game::internal) fn internal_creation_face(&self, card: &str) -> Result<CardFace> {
        // Named token creation selects the native front here; previews and creation share this rule.
        self.catalog.face(card, 0)?;
        Ok(CardFace {
            card: card.into(),
            face: 0,
        })
    }
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
        let zone = if wait.zone == ZoneKind::Field {
            "field"
        } else {
            "ex"
        };
        if self.token_capacity(seat, zone)? != wait.available {
            return Err(invalid("token capacity changed during its decision"));
        }
        let ids = self.internal_create_prints(prints, seat)?;
        self.commit_token_batches(
            execution,
            &wait.instruction,
            vec![(wait.zone, ids)],
            Node::Instruction {
                instruction: Box::new(wait.instruction.clone()),
            },
        )
    }

    pub(in crate::game::internal) fn token_capacity(
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
            .map(|card| {
                let face = self.internal_creation_face(card)?;
                self.new_token_object(
                    &face.card,
                    usize::try_from(face.face).map_err(invalid)?,
                    seat,
                )
            })
            .collect()
    }
}
