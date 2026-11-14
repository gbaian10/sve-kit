//! Public board links retain the authority's participant lifetimes.

use super::{EffectRecipient, ObjectHandle, Viewer};
use crate::game::Game;
use crate::{Result, invalid};

/// An attack whose frozen participants still occupy the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackObservation {
    /// Currently identifiable attacking follower.
    pub attacker: ObjectHandle,
    /// Currently identifiable defending follower or the defending leader.
    pub target: EffectRecipient,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Board observations share the attack authority and current knowledge."
)]
impl Game {
    pub(super) fn observed_attack(&self, perspective: Viewer) -> Result<Option<AttackObservation>> {
        let flow = &self.state.flow;
        if flow["kind"] != "battle" {
            return Ok(None);
        }
        let attacker = flow["attacker"]
            .as_str()
            .ok_or_else(|| invalid("missing attack participant"))?;
        let target = flow["target"]
            .as_str()
            .ok_or_else(|| invalid("missing attack participant"))?;
        let attacker_generation = flow["attacker_generation"]
            .as_u64()
            .ok_or_else(|| invalid("missing attack generation"))?;
        let source = self.object(attacker)?;
        if source.zone != "field" || source.generation != attacker_generation {
            return Ok(None);
        }
        let target = match target {
            "P1.leader" => EffectRecipient::Leader(Viewer::P1),
            "P2.leader" => EffectRecipient::Leader(Viewer::P2),
            id => {
                let generation = flow["target_generation"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing attack generation"))?;
                let object = self.object(id)?;
                if object.zone != "field" || object.generation != generation {
                    return Ok(None);
                }
                EffectRecipient::Card(self.observed_card(perspective, id)?.handle)
            }
        };
        Ok(Some(AttackObservation {
            attacker: self.observed_card(perspective, attacker)?.handle,
            target,
        }))
    }
}
