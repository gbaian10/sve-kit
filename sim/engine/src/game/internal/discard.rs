//! The chooser's hand is exposed only through token-local candidates for this discard.

use crate::game::internal::card::selection::groups::GroupConstraints;
use crate::game::internal::player::CandidateLinks;
use crate::game::player::Viewer;
use crate::game::player::{SelectionGroup, SelectionQuestion};

use super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, contains, invalid, seat_name,
    unsupported,
};
use crate::ir::{
    Action, Bool, Cardinality, Cause, DecisionId, DiscardEvent, DiscardMode, DiscardOutcome,
    Disjointness, Expr, Instruction, InstructionContext, Int, MoveMethod, Nat, ObjectRef,
    PlayerRelation, ReceiptId, SelectionView, Selector, ZoneKind,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct HandWait {
    links: CandidateLinks,
    instruction: Box<Instruction>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
    groups: GroupConstraints,
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Discard { mode, .. } = action {
        match mode {
            DiscardMode::Chosen { .. } => {
                bounds(action)?;
            }
            DiscardMode::All { .. } => {
                eligible(action)?;
            }
            DiscardMode::Random { .. } => {
                random_count(action)?;
            }
            DiscardMode::ChosenCount { .. } => {
                chosen_count(action)?;
            }
            DiscardMode::DownTo { .. } => return Err(unsupported("self hand discard mode")),
        }
    }
    Ok(())
}

/// The discarding player's whole current hand, or only the cards chosen earlier that remain in it.
fn eligible(action: &Action) -> Result<Option<&Selector>> {
    let Action::Discard {
        player,
        mode: DiscardMode::All { eligible },
    } = action
    else {
        return Err(unsupported("self all-hand discard"));
    };
    let (hand, chosen) = match eligible {
        Selector::Intersection { sets } => match sets.as_slice() {
            [hand, chosen @ Selector::Bound { .. }] => (hand, Some(chosen)),
            _ => return Err(unsupported("chosen hand discard")),
        },
        hand => (hand, None),
    };
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = hand
    else {
        return Err(unsupported("direct current hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("unfiltered own hand discard"));
    }
    Ok(chosen)
}

/// A random discard draws from its player's whole current hand; a printed count is positive.
fn random_count(action: &Action) -> Result<&Expr<Int>> {
    let Action::Discard {
        player,
        mode:
            DiscardMode::Random {
                population:
                    Selector::Zone {
                        players,
                        zone: ZoneKind::Hand,
                        view: SelectionView::Current {},
                        predicate,
                        ..
                    },
                count,
            },
    } = action
    else {
        return Err(unsupported("random whole-hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
        || matches!(count, Expr::Literal(Int(value)) if *value <= 0)
    {
        return Err(unsupported("unfiltered random hand discard"));
    }
    Ok(count)
}

/// The player chooses as many cards from their whole current hand as a value read on resolution.
fn chosen_count(action: &Action) -> Result<&Expr<Int>> {
    let Action::Discard {
        player,
        mode:
            DiscardMode::ChosenCount {
                population:
                    Selector::Zone {
                        players,
                        zone: ZoneKind::Hand,
                        view: SelectionView::Current {},
                        predicate,
                        ..
                    },
                count,
            },
    } = action
    else {
        return Err(unsupported("counted whole-hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("unfiltered counted hand discard"));
    }
    Ok(count)
}

fn bounds(action: &Action) -> Result<Cardinality> {
    let Action::Discard {
        player,
        mode: DiscardMode::Chosen { constraint },
    } = action
    else {
        return Err(unsupported("self chosen hand discard"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("single hand discard group"));
    };
    let count = constraint.cardinality;
    let fixed = count.minimum.0 > 0 && count.maximum == Some(count.minimum);
    let any = count.minimum == Nat(0) && count.maximum.is_none();
    if !(fixed || any)
        || group.cardinality != count
        || constraint.disjoint != Disjointness::Disjoint
        || constraint.distinct_by.is_some()
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported(
            "fixed positive or any-number discard cardinality",
        ));
    }
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        predicate,
        ..
    } = &group.selector
    else {
        return Err(unsupported("direct current hand discard"));
    };
    if players.relative_to != *player
        || players.relation != PlayerRelation::SelfPlayer
        || players.cardinality.minimum != Nat(1)
        || players.cardinality.maximum != Some(Nat(1))
        || **predicate != Expr::Literal(Bool(true))
    {
        return Err(unsupported("unfiltered own hand discard"));
    }
    Ok(count)
}

impl HandWait {
    pub(in crate::game::internal) fn player_question(
        &self,
        game: &Game,
        actor: Viewer,
        domain: &super::TargetDomain,
    ) -> Result<SelectionQuestion> {
        let mut question = game.player_selection(
            actor,
            domain,
            &self.members.iter().copied().map(Some).collect::<Vec<_>>(),
            &self.links,
        )?;
        question.groups = self
            .groups
            .members
            .iter()
            .zip(&self.groups.bounds)
            .map(|(members, bounds)| {
                Ok(SelectionGroup {
                    handles: members
                        .iter()
                        .map(|member| {
                            let index = self
                                .members
                                .iter()
                                .position(|candidate| candidate == member)
                                .ok_or_else(|| invalid("missing hand group member"))?;
                            domain
                                .members
                                .get(index)
                                .map(|candidate| candidate.handle)
                                .ok_or_else(|| invalid("missing hand group coordinate"))
                        })
                        .collect::<Result<_>>()?,
                    cardinality: *bounds,
                })
            })
            .collect::<Result<_>>()?;
        Ok(question)
    }

    pub(super) fn strict(
        links: CandidateLinks,
        instruction: Box<Instruction>,
        members: Vec<ObjectRef>,
        bounds: Cardinality,
    ) -> Self {
        Self {
            links,
            instruction,
            groups: GroupConstraints {
                members: vec![members.clone()],
                bounds: vec![bounds],
            },
            members,
            bounds,
        }
    }

    pub(super) const fn grouped(
        links: CandidateLinks,
        instruction: Box<Instruction>,
        members: Vec<ObjectRef>,
        bounds: Cardinality,
        groups: GroupConstraints,
    ) -> Self {
        Self {
            links,
            instruction,
            members,
            bounds,
            groups,
        }
    }

    pub(super) fn record(&self, execution: &mut Execution, members: Vec<ObjectRef>) {
        execution
            .hand_cost_groups
            .insert(self.receipt(), self.groups.clone());
        execution.cost_materials.insert(self.receipt(), members);
    }

    pub(super) const fn proof(&self) -> &GroupConstraints {
        &self.groups
    }

    pub(super) fn witness_members(&self) -> Result<Vec<ObjectRef>> {
        self.groups
            .witness(&self.members, self.bounds)?
            .ok_or_else(|| crate::EngineFailure::IllegalPlay("no hand cost selection".into()))
    }

    pub(super) fn handles(&self, members: &[ObjectRef]) -> Result<Vec<u64>> {
        members
            .iter()
            .map(|member| {
                let index = self
                    .members
                    .iter()
                    .position(|candidate| candidate == member)
                    .ok_or_else(|| invalid("missing hand cost coordinate"))?;
                u64::try_from(index).map_err(invalid)
            })
            .collect()
    }

    pub(super) const fn decision(&self) -> DecisionId {
        DecisionId(self.instruction.id.0)
    }

    pub(super) const fn receipt(&self) -> ReceiptId {
        self.instruction.receipt
    }

    pub(super) fn inline_members(
        &self,
        game: &Game,
        execution: &Execution,
        ids: &[String],
    ) -> Result<Vec<ObjectRef>> {
        self.validate(
            game,
            execution,
            &execution.inline_material_handles(&self.members, ids)?,
        )
        .map_err(|_error| {
            crate::EngineFailure::IllegalPlay("invalid evolution discard selection".into())
        })
    }

    pub(super) fn validate(
        &self,
        game: &Game,
        execution: &Execution,
        handles: &[u64],
    ) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("discard cardinality"));
        }
        let seat = seat_name(execution.player(game, &self.instruction.actor)?)?;
        let mut unique = BTreeSet::new();
        let members = handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate discard handle"));
                }
                let member = *self
                    .members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .ok_or_else(|| invalid("foreign discard handle"))?;
                let object = game.object(execution.object_id(member)?)?;
                if self.instruction.context == InstructionContext::Cost
                    && execution.cost_materials.iter().any(|(receipt, chosen)| {
                        *receipt != self.receipt() && chosen.contains(&member)
                    })
                {
                    return Err(invalid("hand member reserved by another cost"));
                }
                if object.generation != member.generation.0
                    || object.zone != "hand"
                    || object.owner != seat
                    || object.controller != seat
                {
                    return Err(invalid("stale own-hand discard member"));
                }
                Ok(member)
            })
            .collect::<Result<Vec<_>>>()?;
        if !self.groups.accepts(&members) {
            return Err(invalid("discard group constraints"));
        }
        Ok(members)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Private choice and movement use the same authoritative Game."
)]
impl Game {
    pub(super) fn internal_discard_wait(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        if matches!(
            &instruction.action,
            Action::Discard {
                mode: DiscardMode::All { .. },
                ..
            }
        ) {
            let chosen = eligible(&instruction.action)?
                .map(|chosen| execution.objects(self, chosen))
                .transpose()?;
            let player = execution.player(self, &instruction.actor)?;
            let seat = seat_name(player)?;
            let members = self
                .zone_ids(seat, "hand")
                .into_iter()
                .map(|id| execution.reference(self, &id))
                .filter(|member| {
                    chosen.as_ref().is_none_or(|chosen| {
                        member.as_ref().is_ok_and(|member| chosen.contains(member))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let outcome = self.internal_discard_members(execution, &members)?;
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::Discard(outcome));
            return Ok(());
        }
        if let Action::Discard {
            player,
            mode: DiscardMode::Random { .. },
        } = &instruction.action
        {
            let count = execution.integer(self, random_count(&instruction.action)?)?;
            // A value read on resolution may be zero or negative and then discards nothing.
            let count = u64::try_from(count.0).unwrap_or(0);
            let seat = seat_name(execution.player(self, player)?)?;
            let hand = self.zone_ids(seat, "hand");
            // Anonymous hand cards still belong to the population; never draw around them.
            if i64::try_from(hand.len()).map_err(invalid)? != self.zone_count(seat, "hand") {
                return Err(unsupported("random discard from anonymous hand cards"));
            }
            let chosen = self.random_members(hand, usize::try_from(count).map_err(invalid)?)?;
            let members = chosen
                .iter()
                .map(|id| execution.reference(self, id))
                .collect::<Result<Vec<_>>>()?;
            let outcome = self.internal_discard_members(execution, &members)?;
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::Discard(outcome));
            return Ok(());
        }
        let Some(mut count) = self.chosen_discard_bounds(execution, &instruction)? else {
            return Ok(());
        };
        let player = execution.player(self, &instruction.actor)?;
        let seat = seat_name(player)?;
        let mut members = Vec::new();
        let mut candidates = Vec::new();
        for id in self.zone_ids(seat, "hand") {
            let object = self.object(&id)?;
            if object.owner != seat || object.controller != seat {
                return Err(unsupported("foreign hand owner"));
            }
            candidates.push(super::DistributionMember {
                handle: u64::try_from(members.len()).map_err(invalid)?,
                player,
                card: Some(object.card.clone()),
                name: Some(self.card_name(&id)?),
            });
            members.push(execution.reference(self, &id)?);
        }
        count.minimum.0 = count
            .minimum
            .0
            .min(u64::try_from(members.len()).map_err(invalid)?);
        if count.maximum.is_none() {
            count.maximum = Some(Nat(u64::try_from(members.len()).map_err(invalid)?));
        }
        self.internal_wait(
            execution,
            DecisionId(instruction.id.0),
            player,
            InputDomain::Targets(super::TargetDomain {
                members: candidates,
                cardinality: count,
                distinct_names: false,
                original_costs: None,
                roles: vec![],
            }),
            Wait::Discard(HandWait {
                links: CandidateLinks::capture(self, execution, members.iter().copied())?,
                instruction,
                groups: GroupConstraints {
                    members: vec![members.clone()],
                    bounds: vec![count],
                },
                members,
                bounds: count,
            }),
        )
    }

    /// Chosen bounds; a counted discard reads its value now and records nothing for zero or less.
    fn chosen_discard_bounds(
        &self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<Option<Cardinality>> {
        if !matches!(
            &instruction.action,
            Action::Discard {
                mode: DiscardMode::ChosenCount { .. },
                ..
            }
        ) {
            return bounds(&instruction.action).map(Some);
        }
        let requested = execution.integer(self, chosen_count(&instruction.action)?)?;
        let Ok(requested @ 1..) = u64::try_from(requested.0) else {
            execution.receipts.insert(
                instruction.receipt,
                InstructionOutcome::Discard(DiscardOutcome {
                    successful: vec![],
                    moves: vec![],
                }),
            );
            return Ok(None);
        };
        Ok(Some(Cardinality {
            minimum: Nat(requested),
            maximum: Some(Nat(requested)),
        }))
    }

    pub(super) fn internal_discard_answer(
        &mut self,
        execution: &mut Execution,
        wait: &HandWait,
        members: &[ObjectRef],
    ) -> Result<()> {
        let outcome = self.internal_discard_members(execution, members)?;
        execution.receipts.insert(
            wait.instruction.receipt,
            InstructionOutcome::Discard(outcome),
        );
        Ok(())
    }

    pub(super) fn internal_discard_members(
        &mut self,
        execution: &mut Execution,
        members: &[ObjectRef],
    ) -> Result<DiscardOutcome> {
        let mut snapshots = Vec::new();
        let mut ids = Vec::new();
        for &member in members {
            let id = execution.object_id(member)?.to_owned();
            ids.push(id.clone());
            snapshots.push((
                member,
                super::player_id(&self.object(&id)?.controller)?,
                self.internal_hand_characteristics(&id)?,
            ));
        }
        let pending = self.internal_prepare_discard_events(
            &ids,
            execution.context.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            Some(execution),
        )?;
        let (moves, successful) = self.internal_move_members(
            execution,
            members,
            "cemetery",
            ZoneKind::Cemetery,
            MoveMethod::Discard,
            false,
            None,
            None,
        )?;
        let mut events = Vec::new();
        for (before, player, characters) in snapshots {
            if let Some(movement) = moves
                .iter()
                .find(|movement| movement.before == before && movement.after.is_some())
                && let Some(actual_to) = movement.actual_to
            {
                let event = DiscardEvent {
                    player,
                    card_before: before,
                    characters,
                    actual_to,
                };
                execution.record_fact(self, super::Fact::Discard(Box::new(event.clone())));
                events.push((event, movement.after));
            }
        }
        self.internal_commit_discard_events(pending, &events)?;
        Ok(DiscardOutcome { successful, moves })
    }
}
