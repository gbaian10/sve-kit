//! Public revelation is local to one resolution and one hidden location.

use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::EngineFailure;
use crate::ir::{
    Bool, Expr, Instruction, MemberProjection, Nat, NumericProperty, ObjectReceipt, ObjectRef,
    PlayerRef, PlayerRelation, ReceiptId, ReceiptLocation, SelectionView, Selector, ZoneKind,
};
use core::mem::take;

#[derive(Clone, Debug)]
pub(super) struct PublicReveal {
    object: ObjectRef,
    id: String,
    controller: String,
    zone: String,
    viewers: Vec<String>,
}

pub(super) fn check_objects(objects: &Selector) -> Result<()> {
    if matches!(objects, Selector::Bound { .. }) || look_receipt(objects).is_some() {
        return Ok(());
    }
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = objects
    else {
        return Err(unsupported("direct whole-hand reveal"));
    };
    if players.relative_to != (PlayerRef::Controller {})
        || !matches!(
            players.relation,
            PlayerRelation::SelfPlayer | PlayerRelation::Opponents
        )
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("one unfiltered hand reveal"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Scoped revelation shares the authoritative Game visibility state."
)]
impl Game {
    pub(super) fn internal_reveal(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<()> {
        check_objects(objects)?;
        let (members, selected) = match objects {
            Selector::Zone { players, .. } => {
                let mut members = Vec::new();
                for player in execution.players(self, players)? {
                    for id in self.zone_ids(seat_name(player)?, "hand") {
                        members.push(execution.reference(self, &id)?);
                    }
                }
                (members, false)
            }
            Selector::Bound { .. } => (execution.objects(self, objects)?, true),
            Selector::ReceiptMembers { .. } => (
                execution.observed_objects(self, objects, execution.context.controller)?,
                true,
            ),
            _ => return Err(unsupported("whole hand reveal")),
        };
        self.internal_reveal_members(
            execution,
            instruction,
            &members,
            selected.then_some(objects),
        )
    }

    pub(super) fn internal_reveal_members(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        members: &[ObjectRef],
        selected: Option<&Selector>,
    ) -> Result<()> {
        let zone = if selected.is_some_and(|objects| look_receipt(objects).is_some()) {
            "deck"
        } else {
            "hand"
        };
        let mut ids = Vec::new();
        let mut current_members = Vec::with_capacity(members.len());
        for member in members {
            let id = execution.object_id(*member)?.to_owned();
            let object = self.object(&id)?;
            // Look choices deliberately reveal their selected card after moving it to hand.
            if selected.is_some_and(|objects| {
                object.generation != member.generation.0
                    && execution.is_private_hand_target(objects, *member)
            }) {
                continue;
            }
            if (zone == "deck" && object.generation != member.generation.0)
                || object.zone != zone
                || object.controller != object.owner
                || (selected.is_some()
                    && object.controller != seat_name(execution.context.controller)?)
            {
                return Err(invalid("foreign hand reveal member"));
            }
            let current_reference = execution.reference(self, &id)?;
            self.internal_record_reveal(execution, current_reference, &id)?;
            ids.push(id);
            current_members.push(current_reference);
        }
        self.reveal(&ids, "all", &execution.frame(), true)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Revealed(current_members),
        );
        Ok(())
    }

    pub(super) fn internal_record_reveal(
        &self,
        execution: &mut Execution,
        reference: ObjectRef,
        id: &str,
    ) -> Result<()> {
        let object = self.object(id)?;
        if object.generation != reference.generation.0
            || !matches!(object.zone.as_str(), "hand" | "deck")
        {
            return Err(invalid("current hidden reveal generation"));
        }
        let viewers = ["P1", "P2"]
            .into_iter()
            .filter(|viewer| {
                self.state
                    .knowledge
                    .get(*viewer)
                    .is_none_or(|known| !known.knows_location(id))
            })
            .map(str::to_owned)
            .collect();
        execution.reveals.push(PublicReveal {
            object: reference,
            id: id.to_owned(),
            controller: object.controller.clone(),
            zone: object.zone.clone(),
            viewers,
        });
        Ok(())
    }

    pub(super) fn internal_reveal_expiry(&mut self, execution: &mut Execution, end: bool) {
        let mut active = Vec::new();
        for grant in take(&mut execution.reveals) {
            let Some(current) = self.state.objects.get(&grant.id) else {
                for viewer in grant.viewers {
                    if let Some(known) = self.state.knowledge.get_mut(&viewer) {
                        known.forget_location(&grant.id);
                    }
                }
                continue;
            };
            let same_location = current.generation == grant.object.generation.0
                && current.controller == grant.controller
                && current.zone == grant.zone;
            if !end && same_location {
                active.push(grant);
                continue;
            }
            if matches!(current.zone.as_str(), "hand" | "deck" | "evolve_deck") {
                let owner = current.owner.clone();
                let own_hand = current.zone == "hand";
                for viewer in grant.viewers {
                    if !(own_hand && viewer == owner)
                        && let Some(known) = self.state.knowledge.get_mut(&viewer)
                    {
                        // Remember the printed card, but never locate its later private slot.
                        known.forget_location(&grant.id);
                    }
                }
            }
        }
        execution.reveals = active;
    }
}

pub(super) const fn look_receipt(objects: &Selector) -> Option<ReceiptId> {
    if let Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look { receipt },
        projection: MemberProjection::Before,
    } = objects
        && let ReceiptLocation::Local { id } = receipt.location()
    {
        Some(*id)
    } else {
        None
    }
}

pub(super) const fn look_sum(objects: &Selector, property: NumericProperty) -> Option<ReceiptId> {
    if matches!(
        property,
        NumericProperty::Cost | NumericProperty::OriginalCost
    ) {
        look_receipt(objects)
    } else {
        None
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Revelation authority is shared by value and movement consumers."
)]
impl Execution {
    pub(super) fn public_look_objects(
        &self,
        game: &Game,
        objects: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        let id = look_receipt(objects).ok_or_else(|| unsupported("public look receipt"))?;
        let Some(InstructionOutcome::Look(outcome)) = self.receipts.get(&id) else {
            return Err(invalid("missing public look"));
        };
        if self.look_viewers.get(&id) != Some(&self.context.controller) {
            return Err(EngineFailure::AuthorizationIntegrity(
                "foreign public look".into(),
            ));
        }
        let members: Vec<_> = outcome.looked.iter().map(|view| view.object).collect();
        let published = self.receipts.values().any(|result| {
            matches!(result, InstructionOutcome::Revealed(revealed) if revealed == &members)
        });
        if !published {
            return Err(EngineFailure::AuthorizationIntegrity(
                "look was not revealed".into(),
            ));
        }
        for member in &members {
            let object = game.object(self.object_id(*member)?)?;
            if object.generation != member.generation.0
                || object.zone != "deck"
                || object.owner != seat_name(self.context.controller)?
                || object.controller != object.owner
            {
                return Err(unsupported("stale public look member"));
            }
        }
        Ok(members)
    }

    pub(super) fn public_look_sum(
        &self,
        game: &Game,
        objects: &Selector,
        property: NumericProperty,
    ) -> Result<i64> {
        look_sum(objects, property).ok_or_else(|| unsupported("public look cost"))?;
        super::aggregates::checked_sum(
            self.public_look_objects(game, objects)?
                .into_iter()
                .map(|member| {
                    super::aggregates::numeric_value(game, self.object_id(member)?, property)
                }),
            "revealed cost overflow",
        )
    }

    pub(super) fn public_look_member(
        &self,
        game: &Game,
        objects: &Selector,
        member: ObjectRef,
    ) -> Result<bool> {
        if look_receipt(objects).is_none() {
            return Ok(false);
        }
        let members = self.public_look_objects(game, objects)?;
        let id = self.object_id(member)?;
        Ok(members.contains(&member)
            && ["P1", "P2"].iter().all(|seat| {
                game.state
                    .knowledge
                    .get(*seat)
                    .is_some_and(|known| known.knows_location(id))
            }))
    }
}
