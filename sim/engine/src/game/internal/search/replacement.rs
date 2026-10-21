//! Found-local cost conditions never authorize general reads of a private deck.

use super::{
    Action, Binding, Bool, CharacteristicView, Execution, Expr, Game, InputDomain, Instruction,
    MoveDestination, ObjectRef, OwnerZone, PlayerId, Predicate, Result, SearchDestination,
    SearchOutcome, SnapshotId, Wait, candidate_view, invalid, seat_name, unsupported,
};
use crate::ir::{Comparison, Int, IntOperation, NumberRead, SearchFound, SearchReplacementAnswer};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ReplacementWait {
    instruction: Box<Instruction>,
    player: PlayerId,
    outcome: SearchOutcome,
    deck: Vec<String>,
    snapshot: SnapshotId,
}

pub(super) fn check_predicate(value: &Expr<Bool>, binding: Binding<SearchFound>) -> Result<()> {
    match value {
        Expr::Literal(_) => Ok(()),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::SearchFoundAny {
                found,
                object,
                predicate,
            } if *found == binding => cost_limit(predicate, *object).map(|_| ()),
            _ => Err(unsupported("search replacement predicate")),
        },
        _ => Err(unsupported("search replacement predicate")),
    }
}

fn cost_limit(value: &Expr<Bool>, binding: Binding<ObjectRef>) -> Result<i64> {
    if let Expr::Compute(predicate) = value
        && let Predicate::CompareInt {
            left: Expr::Compute(operation),
            comparison: Comparison::LessEqual,
            right: Expr::Literal(Int(limit)),
        } = predicate.as_ref()
        && let IntOperation::InformationOrZero {
            information:
                NumberRead::Cost {
                    object,
                    view: CharacteristicView::Effective,
                },
        } = operation.as_ref()
        && candidate_view(object, binding)
    {
        Ok(*limit)
    } else {
        Err(unsupported(
            "search replacement found-local cost upper bound",
        ))
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The optional continuation shares Search's one movement and shuffle path."
)]
impl Game {
    pub(super) fn internal_search_replacement_validate(
        &self,
        execution: &Execution,
        found: &[ObjectRef],
        predicate: &Expr<Bool>,
        destination: &MoveDestination,
        player: PlayerId,
    ) -> Result<()> {
        if !found.is_empty() && self.search_replacement_applies(execution, found, predicate)? {
            self.internal_search_check_move(execution, found, destination, player)?;
            let (zone, _) = self.internal_search_destination(execution, destination, player)?;
            if self.zone_count(seat_name(player)?, zone) >= 5 {
                return Err(unsupported(
                    "search optional replacement needs two executable destinations",
                ));
            }
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_search_replacement(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        player: PlayerId,
        mut outcome: SearchOutcome,
    ) -> Result<()> {
        let Action::Search {
            destination:
                Some(SearchDestination::OptionalSelfReplacement {
                    decision,
                    predicate,
                    movement,
                }),
            ..
        } = &instruction.action
        else {
            return Err(unsupported("search optional replacement"));
        };
        let found = outcome
            .found
            .iter()
            .map(|view| view.object)
            .collect::<Vec<_>>();
        if found.is_empty() || !self.search_replacement_applies(execution, &found, predicate)? {
            outcome.moved = self.internal_search_move(
                execution,
                &found,
                &MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                },
                player,
            )?;
            return self.internal_search_finish(execution, instruction, player, outcome);
        }
        // Both alternatives must be executable before asking; previews cannot mutate the authority.
        for destination in [
            &MoveDestination::Owners {
                zone: OwnerZone::Hand,
            },
            &movement.to,
        ] {
            let mut preview = self.clone();
            let results = preview.internal_search_move(
                &mut execution.clone(),
                &found,
                destination,
                player,
            )?;
            if results.iter().any(|result| result.after.is_none()) {
                return Err(unsupported(
                    "search optional replacement needs two executable destinations",
                ));
            }
        }
        let wait = ReplacementWait {
            instruction: Box::new(instruction.clone()),
            player,
            outcome,
            deck: self.internal_known_deck(seat_name(player)?)?,
            snapshot: SnapshotId(self.state.ir_epoch),
        };
        self.internal_wait(
            execution,
            *decision,
            player,
            InputDomain::SearchReplacement,
            Wait::SearchReplacement(wait),
        )
    }

    fn search_replacement_applies(
        &self,
        execution: &Execution,
        found: &[ObjectRef],
        value: &Expr<Bool>,
    ) -> Result<bool> {
        match value {
            Expr::Literal(Bool(value)) => Ok(*value),
            Expr::Compute(operation) => {
                let Predicate::SearchFoundAny {
                    object, predicate, ..
                } = operation.as_ref()
                else {
                    return Err(unsupported("search replacement predicate"));
                };
                let limit = cost_limit(predicate, *object)?;
                for &reference in found {
                    let id = execution.object_id(reference)?;
                    let cost = self.internal_effective_cost(id)?;
                    if cost <= limit {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Err(unsupported("search replacement predicate")),
        }
    }

    pub(in crate::game::internal) fn internal_search_replacement_answer(
        &mut self,
        execution: &mut Execution,
        wait: &ReplacementWait,
        answer: SearchReplacementAnswer,
    ) -> Result<()> {
        if self.internal_known_deck(seat_name(wait.player)?)? != wait.deck
            || self.state.ir_epoch
                != wait
                    .snapshot
                    .0
                    .checked_add(1)
                    .ok_or_else(|| invalid("replacement epoch overflow"))?
        {
            return Err(invalid("search replacement snapshot changed"));
        }
        let mut found = Vec::new();
        for view in &wait.outcome.found {
            if self.object(execution.object_id(view.object)?)?.generation
                != view.object.generation.0
            {
                return Err(invalid("stale search replacement generation"));
            }
            found.push(view.object);
        }
        let Action::Search {
            destination: Some(SearchDestination::OptionalSelfReplacement { movement, .. }),
            ..
        } = &wait.instruction.action
        else {
            return Err(unsupported("search replacement movement"));
        };
        let destination = match answer {
            SearchReplacementAnswer::Original => &MoveDestination::Owners {
                zone: OwnerZone::Hand,
            },
            SearchReplacementAnswer::Replacement => &movement.to,
        };
        let moved = self.internal_search_move(execution, &found, destination, wait.player)?;
        self.internal_search_finish(
            execution,
            &wait.instruction,
            wait.player,
            SearchOutcome {
                moved,
                ..wait.outcome.clone()
            },
        )
    }
}
