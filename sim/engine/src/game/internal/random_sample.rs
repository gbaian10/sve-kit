//! A hand sample stays private until an explicit Reveal consumes its frozen generations.
use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Action, Cardinality, Expr, Instruction, InstructionContext, Int, MemberProjection, Nat, Node,
    ObjectReceipt, PlayerRef, PlayerRelation, PlayerSet, RandomnessPolicy, ReceiptId,
    ReceiptLocation, Selector, StepwiseMode, TextQuantifier, Visibility, ZoneKind,
};

pub(super) fn check(instruction: &Instruction) -> Result<()> {
    let Action::Random {
        population,
        rng: RandomnessPolicy::FreshRequest,
        visibility,
        ..
    } = &instruction.action
    else {
        return Err(unsupported("fresh hand sampling"));
    };
    let audience = Visibility::Only {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
    };
    if instruction.context != InstructionContext::Effect
        || instruction.precondition.is_some()
        || *visibility != audience
        || !matches!(
            population,
            Selector::Zone {
                zone: ZoneKind::Hand,
                ..
            }
        )
    {
        return Err(unsupported("private sample from one complete hand"));
    }
    super::reveal::check_objects(population)
}

pub(super) const fn receipt(objects: &Selector) -> Option<ReceiptId> {
    let Selector::ReceiptMembers {
        receipt: ObjectReceipt::Random { receipt },
        projection: MemberProjection::Before,
    } = objects
    else {
        return None;
    };
    match receipt.location() {
        ReceiptLocation::Local { id } => Some(*id),
        ReceiptLocation::Iteration { .. } => None,
    }
}

pub(super) fn mask(node: &mut Node) -> Result<()> {
    if let Node::Instruction { instruction } = node
        && matches!(instruction.action, Action::Random { .. })
    {
        check(instruction)?;
        instruction.action = Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(0)),
            mode: StepwiseMode::Exact,
        };
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Hand sampling uses the authoritative RNG and receipt store."
)]
impl Game {
    pub(super) fn internal_random_sample(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check(instruction)?;
        let Action::Random {
            population: Selector::Zone { players, .. },
            count,
            ..
        } = &instruction.action
        else {
            return Err(invalid("checked hand sample"));
        };
        let players = execution.players(self, players)?;
        let [player] = players.as_slice() else {
            return Err(invalid("one hand sample player"));
        };
        let seat = seat_name(*player)?;
        let hand = self.zone_ids(seat, "hand");
        // Anonymous members belong to the population; drawing around them would bias the sample.
        if i64::try_from(hand.len()).map_err(invalid)? != self.zone_count(seat, "hand") {
            return Err(unsupported("sampling anonymous hand members"));
        }
        let chosen = self.random_members(hand, usize::try_from(count.0).unwrap_or(usize::MAX))?;
        let members = chosen
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Sampled(members));
        Ok(())
    }
}
