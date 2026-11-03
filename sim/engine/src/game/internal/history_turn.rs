//! Current-turn queries read committed public facts; only Combo projects a pending formal play.

use super::{Execution, Fact, Game, Nat, Result, invalid, seat_name, unsupported};
use crate::ir::PlayerId;
use crate::ir::events::life::LifeFilter;
use crate::ir::events::subject::SubjectInformation;
use crate::ir::events::timing::opponent;
use crate::ir::history_turn::{PlayedBy, life_plan, life_sum_plan, played_by};
use crate::ir::history_turn::{SubjectHistoryKind, subject_plan};
use crate::ir::{CardKind, CardName, ClassId, HistoryCount, Int, NumericHistoryQuery, TraitId};
use crate::ir::{ExecutionRef, HistoryQuery, PlayerRef, Requirement, Requirements};
use alloc::collections::BTreeSet;
use serde::{Deserialize, Serialize};

pub(super) fn public_requirement(requirements: &Requirements, requirement: &Requirement) -> bool {
    requirement.locations.iter().all(|location| {
        location
            .ir_path
            .strip_suffix("/query/pattern")
            .or_else(|| {
                location
                    .ir_path
                    .split_once("/query/pattern/")
                    .map(|(read, _)| read)
            })
            .or_else(|| location.ir_path.strip_suffix("/query/predicate"))
            .or_else(|| {
                location
                    .ir_path
                    .split_once("/query/predicate/")
                    .map(|(read, _)| read)
            })
            .is_some_and(|read| {
                requirements.required.iter().any(|other| {
                    matches!(
                        other.capability.code,
                        "read.history.count" | "read.history.sum"
                    ) && other.locations.iter().any(|at| at.ir_path == read)
                })
            })
    })
}

fn count(execution: &Execution, game: &Game, player: &PlayerRef, pending: bool) -> Result<Nat> {
    let player = execution.player(game, player)?;
    let count = game
        .state
        .counters
        .get(&format!("{}.cards_played", seat_name(player)?))
        .copied()
        .unwrap_or_default();
    let mut count = u64::try_from(count).map_err(invalid)?;
    // CR 13.2 includes the card whose formal play is being checked, before it commits.
    if pending
        && player == execution.context.controller
        && (execution.spell || execution.field_entry)
        && !execution.facts.iter().any(|fact| {
            matches!(fact,
            Fact::CardPlayed { execution: id, .. } if *id == execution.id)
        })
    {
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("Combo overflow"))?;
    }
    Ok(Nat(count))
}

pub(super) fn combo(
    execution: &Execution,
    game: &Game,
    player: &PlayerRef,
    reference: ExecutionRef,
) -> Result<Nat> {
    if reference != (ExecutionRef::Current {}) {
        return Err(unsupported("Combo execution"));
    }
    count(execution, game, player, true)
}

pub(super) fn played(execution: &Execution, game: &Game, query: &HistoryQuery) -> Result<Nat> {
    if let PlayedBy::Player(player) = played_by(query)
        .ok_or_else(|| unsupported("unfiltered current-turn played-card history"))?
    {
        count(execution, game, &player, false)
    } else {
        let own = count(execution, game, &PlayerRef::Controller {}, false)?.0;
        let other = count(execution, game, &opponent(), false)?.0;
        own.checked_add(other)
            .map(Nat)
            .ok_or_else(|| invalid("played-card count overflow"))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::game) enum SubjectHistoryEvent {
    Attack,
    Evolve,
    Leave,
}

/// Only public characteristics needed by the admitted current-turn queries are retained.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct SubjectHistoryFact {
    pub(in crate::game::internal) event: SubjectHistoryEvent,
    pub(in crate::game::internal) destination: Option<String>,
    pub(in crate::game::internal) object: u64,
    pub(in crate::game::internal) generation: u64,
    pub(in crate::game::internal) controller: u32,
    pub(in crate::game::internal) active: u32,
    pub(in crate::game::internal) kind: String,
    pub(in crate::game::internal) class: String,
    pub(in crate::game::internal) traits: Vec<String>,
    pub(in crate::game::internal) names: Option<Vec<String>>,
    pub(in crate::game::internal) token: bool,
    pub(in crate::game::internal) destroyed: bool,
}

pub(super) fn subjects(
    execution: &Execution,
    game: &Game,
    query: &HistoryQuery,
    count: HistoryCount,
) -> Result<Nat> {
    if let Some(filter) = life_plan(query) {
        if count != HistoryCount::Facts {
            return Err(unsupported("life history subject count"));
        }
        return u64::try_from(life_counter(
            execution,
            game,
            filter,
            "leader_hp_decreased",
        )?)
        .map(Nat)
        .map_err(invalid);
    }
    let Some((event, filter)) = subject_plan(query) else {
        if count != HistoryCount::Facts {
            return Err(unsupported("played history subject count"));
        }
        return played(execution, game, query);
    };
    let mut facts = 0_u64;
    let mut identities = BTreeSet::new();
    for fact in &game.state.ir_subject_history {
        let eligible = match event {
            SubjectHistoryKind::Attack => fact.event == SubjectHistoryEvent::Attack,
            SubjectHistoryKind::Evolve => fact.event == SubjectHistoryEvent::Evolve,
            SubjectHistoryKind::Destroy => {
                fact.event == SubjectHistoryEvent::Leave && fact.destroyed
            }
            SubjectHistoryKind::Leave => fact.event == SubjectHistoryEvent::Leave,
            SubjectHistoryKind::Cemetery => {
                fact.event == SubjectHistoryEvent::Leave
                    && fact.destination.as_deref() == Some("cemetery")
            }
            SubjectHistoryKind::Hand => {
                fact.event == SubjectHistoryEvent::Leave
                    && fact.destination.as_deref() == Some("hand")
            }
        };
        if !eligible {
            continue;
        }
        let kind = match fact.kind.as_str() {
            "follower" => CardKind::Follower,
            "spell" => CardKind::Spell,
            "amulet" => CardKind::Amulet,
            _ => return Err(invalid("invalid saved history kind")),
        };
        let class = ClassId(fact.class.clone());
        let traits = fact.traits.iter().cloned().map(TraitId).collect::<Vec<_>>();
        if filter.name.is_some() && fact.names.is_none() {
            return Err(unsupported("unadmitted historical subject names"));
        }
        let names = fact
            .names
            .iter()
            .flatten()
            .cloned()
            .map(CardName)
            .collect::<Vec<_>>();
        let information = SubjectInformation {
            kind,
            class: Some(&class),
            traits: &traits,
            names: &names,
        };
        let itself = execution.ability.is_some_and(|source| {
            source.holder.id.0 == fact.object && source.holder.generation.0 == fact.generation
        });
        if filter.matches_information(
            &information,
            itself,
            execution.context.controller,
            PlayerId {
                seat: fact.controller,
            },
            PlayerId { seat: fact.active },
            fact.token,
        ) {
            facts = facts
                .checked_add(1)
                .ok_or_else(|| invalid("history count overflow"))?;
            identities.insert((fact.object, fact.generation));
        }
    }
    Ok(Nat(match count {
        HistoryCount::Facts => facts,
        HistoryCount::Subjects => u64::try_from(identities.len()).map_err(invalid)?,
    }))
}

fn life_counter(
    execution: &Execution,
    game: &Game,
    filter: LifeFilter,
    counter: &str,
) -> Result<i64> {
    let player = match filter.own {
        Some(true) => PlayerRef::Controller {},
        Some(false) => opponent(),
        None => return Err(unsupported("life history player")),
    };
    let seat = seat_name(execution.player(game, &player)?)?;
    let value = game
        .state
        .counters
        .get(&format!("{seat}.{counter}"))
        .copied()
        .unwrap_or_default();
    if value < 0 {
        return Err(invalid("negative life history"));
    }
    Ok(value)
}

pub(super) fn life_sum(
    execution: &Execution,
    game: &Game,
    query: &NumericHistoryQuery,
) -> Result<Int> {
    let filter = life_sum_plan(query).ok_or_else(|| unsupported("life history sum"))?;
    life_counter(execution, game, filter, "leader_hp_increased").map(Int)
}
