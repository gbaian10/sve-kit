//! Current-turn queries read committed public facts; only Combo projects a pending formal play.

mod commit;

use super::observation::ObjectLink;
use super::{Execution, Fact, Game, Nat, Result, invalid, seat_name, unsupported};
use crate::game::player::Viewer;
use crate::game::player::observation::TurnHistory;
use crate::ir::events::change::StatFilter;
use crate::ir::events::life::LifeFilter;
use crate::ir::events::subject::{SubjectFilter, SubjectInformation};
use crate::ir::events::timing::opponent;
use crate::ir::history_turn::public::{banished_condition, damaged_plan};
use crate::ir::history_turn::source_link::SourceLink;
use crate::ir::history_turn::{
    HistoryPlan, NumericHistoryPlan, PlayedBy, PlayedHistoryFilter, SubjectHistoryKind,
};
use crate::ir::{Action, Bool, Control, Expr, Node, Program};
use crate::ir::{CardKind, CardName, ClassId, HistoryCount, Int, Selector, TraitId};
use crate::ir::{ExecutionRef, Location, PlayerRef, Requirement, Requirements};
use crate::ir::{ObjectOperand, ObjectRef, PlayerId, Stat};
use alloc::collections::BTreeSet;
use core::slice::from_ref;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct DiceHistoryFact {
    pub(super) turn: u64,
    pub(super) player: u32,
    pub(super) value: u64,
}

fn saved_kind(kind: &str) -> Result<CardKind> {
    match kind {
        "follower" => Ok(CardKind::Follower),
        "spell" => Ok(CardKind::Spell),
        "amulet" => Ok(CardKind::Amulet),
        _ => Err(invalid("invalid saved history kind")),
    }
}

/// The current-turn stat history only needs the positive facets for each committed event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct StatGainHistoryFact {
    link: Option<ObjectLink>,
    object: u64,
    generation: u64,
    attack: bool,
    life: bool,
}

fn stat_gain_exists(execution: &Execution, game: &Game, plan: &StatFilter) -> Result<Bool> {
    let holder = execution.object(game, &ObjectOperand::SourceHolder {})?;
    Ok(Bool(game.state.ir_stat_gain_history.iter().any(|fact| {
        fact.object == holder.id.0
            && fact.generation == holder.generation.0
            && plan.properties.iter().any(|property| match property {
                Stat::Attack => fact.attack,
                Stat::Life => fact.life,
            })
    })))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Public history observations share the game state."
)]
impl Game {
    pub(in crate::game) fn observed_history(&self, perspective: Viewer) -> Result<TurnHistory> {
        use crate::game::player::observation::{
            DamageHistory, HistoryEvent, StatGainHistory, SubjectHistory, TurnHistory, viewer, zone,
        };
        let subjects = self
            .state
            .ir_subject_history
            .iter()
            .map(|fact| {
                Ok(SubjectHistory {
                    event: match fact.event {
                        SubjectHistoryEvent::Play => HistoryEvent::Play,
                        SubjectHistoryEvent::Attack => HistoryEvent::Attack,
                        SubjectHistoryEvent::Evolve => HistoryEvent::Evolve,
                        SubjectHistoryEvent::Leave => HistoryEvent::Leave,
                    },
                    current: fact.link.observed_public(self, perspective),
                    controller: viewer(seat_name(PlayerId {
                        seat: fact.controller,
                    })?)?,
                    active: viewer(seat_name(PlayerId { seat: fact.active })?)?,
                    kind: saved_kind(&fact.kind)?,
                    class: (!fact.class.is_empty()).then(|| fact.class.clone()),
                    traits: fact.traits.clone(),
                    names: fact.names.clone(),
                    token: fact.token,
                    destroyed: fact.destroyed,
                    destination: fact.destination.as_deref().map(zone).transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let damage = self
            .state
            .ir_damage_history
            .iter()
            .map(|fact| {
                Ok(DamageHistory {
                    recipient: fact
                        .recipient_link
                        .as_ref()
                        .and_then(|link| link.observed_public(self, perspective)),
                    source: fact
                        .source_link
                        .as_ref()
                        .and_then(|link| link.observed_public(self, perspective)),
                    controller: viewer(seat_name(PlayerId {
                        seat: fact.controller,
                    })?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let stat_gains = self
            .state
            .ir_stat_gain_history
            .iter()
            .map(|fact| StatGainHistory {
                current: fact
                    .link
                    .as_ref()
                    .and_then(|link| link.observed_public(self, perspective)),
                attack: fact.attack,
                life: fact.life,
            })
            .collect();
        let counters = |name| {
            Ok([
                committed_counter(self, PlayerId { seat: 1 }, name)?,
                committed_counter(self, PlayerId { seat: 2 }, name)?,
            ])
        };
        Ok(TurnHistory {
            subjects,
            damage,
            stat_gains,
            dice: self.observed_dice_history()?,
            played: counters("cards_played")?,
            union_bursts: counters("ub_activated")?,
        })
    }
}

pub(super) fn public_requirement(requirements: &Requirements, requirement: &Requirement) -> bool {
    requirement
        .locations
        .iter()
        .all(|location| public_location(requirements, location))
}

pub(super) fn public_location(requirements: &Requirements, location: &Location) -> bool {
    location
        .ir_path
        .strip_suffix("/query/include_execution")
        .or_else(|| {
            location
                .ir_path
                .strip_suffix("/query/include_execution/execution")
        })
        .or_else(|| location.ir_path.strip_suffix("/query/pattern"))
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
}

pub(super) fn check_lesson_condition(game: &Game, condition: &Expr<Bool>) -> Result<()> {
    if let Some(name) = banished_condition(condition) {
        check_lesson_resource(game, name)?;
    }
    Ok(())
}

fn check_lesson_resource(game: &Game, name: &CardName) -> Result<()> {
    if !game.catalog.rule_bindings.has_resource("lesson_item")
        || !game
            .catalog
            .rule_bindings
            .matches_resource("lesson_item", from_ref(&name.0))?
    {
        return Err(unsupported("history name is not the bound lesson resource"));
    }
    Ok(())
}

pub(super) fn exists(execution: &Execution, game: &Game, plan: &HistoryPlan<'_>) -> Result<Bool> {
    match plan {
        HistoryPlan::StatGain(filter) => stat_gain_exists(execution, game, filter),
        HistoryPlan::Lesson(name) => {
            check_lesson_resource(game, name)?;
            Ok(Bool(
                committed_counter(game, execution.context.controller, "magic_item_banished")? > 0,
            ))
        }
        HistoryPlan::Dice(_)
        | HistoryPlan::Discarded
        | HistoryPlan::Life(_)
        | HistoryPlan::Ub { .. }
        | HistoryPlan::Played(_)
        | HistoryPlan::PlayedFiltered { .. }
        | HistoryPlan::Subject { .. } => Err(unsupported("boolean expression")),
    }
}

pub(super) fn mask_public_history(program: &mut Program) {
    for step in &mut program.steps {
        if let Node::Control {
            control: Control::If { condition, .. },
        } = &mut step.node
            && banished_condition(condition).is_some()
        {
            *condition = Expr::Literal(Bool(false));
        }
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Destroy { objects } = &mut instruction.action
            && damaged_plan(objects).is_some()
        {
            *objects = Selector::Objects { objects: vec![] };
        }
    }
}

fn committed_counter(game: &Game, player: PlayerId, name: &str) -> Result<u64> {
    let count = game
        .state
        .counters
        .get(&format!("{}.{name}", seat_name(player)?))
        .copied()
        .unwrap_or_default();
    u64::try_from(count).map_err(invalid)
}

fn played_count(
    execution: &Execution,
    game: &Game,
    player: &PlayerRef,
    pending: bool,
) -> Result<Nat> {
    let player = execution.player(game, player)?;
    let mut count = committed_counter(game, player, "cards_played")?;
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
    played_count(execution, game, player, true)
}

fn filtered_played(execution: &Execution, game: &Game, plan: &PlayedHistoryFilter) -> Result<Nat> {
    let controller = execution.context.controller;
    let all = game
        .state
        .ir_subject_history
        .iter()
        .filter(|fact| {
            fact.event == SubjectHistoryEvent::Play && fact.controller == controller.seat
        })
        .collect::<Vec<_>>();
    if u64::try_from(all.len()).map_err(invalid)?
        != committed_counter(game, controller, "cards_played")?
    {
        return Err(unsupported("incomplete filtered played-card history"));
    }
    let filters = plan.subjects();
    let mut count = 0_u64;
    for fact in all {
        if filters.iter().any(|filter| filter.name.is_some()) && fact.names.is_none() {
            return Err(unsupported("unadmitted historical subject names"));
        }
        let kind = saved_kind(&fact.kind)?;
        let traits = fact.traits.iter().cloned().map(TraitId).collect::<Vec<_>>();
        let names = fact
            .names
            .iter()
            .flatten()
            .cloned()
            .map(CardName)
            .collect::<Vec<_>>();
        let information = SubjectInformation {
            original_cost: None,
            keywords: &[],
            kind: Some(kind),
            traits: &traits,
            names: &names,
            class: None,
        };
        if filters.iter().any(|filter| {
            filter.matches_information(
                &information,
                false,
                controller,
                controller,
                PlayerId { seat: fact.active },
                fact.token,
            )
        }) {
            count = count
                .checked_add(1)
                .ok_or_else(|| invalid("played-card count overflow"))?;
        }
    }
    Ok(Nat(count))
}

/// Only committed IR executions can exclude their own fact from the shared counter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct UbHistoryFact {
    pub(in crate::game::internal) execution: u64,
    pub(in crate::game::internal) controller: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::game) enum SubjectHistoryEvent {
    Play,
    Attack,
    Evolve,
    Leave,
}

/// Only public characteristics needed by the admitted current-turn queries are retained.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct SubjectHistoryFact {
    pub(in crate::game::internal) link: ObjectLink,
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

fn ub_count(
    execution: &Execution,
    game: &Game,
    player: Option<&PlayerRef>,
    exclude_current: bool,
) -> Result<Nat> {
    let player = player
        .map(|player| execution.player(game, player))
        .transpose()?;
    let mut total = if let Some(player) = player {
        committed_counter(game, player, "ub_activated")?
    } else {
        let own = committed_counter(game, execution.context.controller, "ub_activated")?;
        let other = committed_counter(game, execution.player(game, &opponent())?, "ub_activated")?;
        own.checked_add(other)
            .ok_or_else(|| invalid("UB history overflow"))?
    };
    // The counter includes prototype plays and explicit starting history as well as IR plays.
    if exclude_current
        && game.state.ir_ub_history.iter().any(|fact| {
            fact.execution == execution.id.0
                && player.is_none_or(|player| fact.controller == player.seat)
        })
    {
        total = total
            .checked_sub(1)
            .ok_or_else(|| invalid("UB counter omits its committed execution"))?;
    }
    Ok(Nat(total))
}

pub(super) fn count(
    execution: &Execution,
    game: &Game,
    plan: &HistoryPlan<'_>,
    count: HistoryCount,
) -> Result<Nat> {
    match plan {
        HistoryPlan::Dice(filter) => {
            if count != HistoryCount::Facts {
                return Err(unsupported("dice history subject count"));
            }
            super::dice::history_count(execution, game, filter.face)
        }
        HistoryPlan::Discarded => {
            if count != HistoryCount::Facts {
                return Err(unsupported("discard history subject count"));
            }
            committed_counter(game, execution.context.controller, "discarded").map(Nat)
        }
        HistoryPlan::Life(filter) => {
            if count != HistoryCount::Facts {
                return Err(unsupported("life history subject count"));
            }
            let counter = if filter.up == Some(true) {
                "leader_hp_increase_count"
            } else {
                "leader_hp_decreased"
            };
            u64::try_from(life_counter(execution, game, *filter, counter)?)
                .map(Nat)
                .map_err(invalid)
        }
        HistoryPlan::Ub {
            player,
            exclude_current,
        } => {
            if count != HistoryCount::Facts {
                return Err(unsupported("UB history subject count"));
            }
            ub_count(execution, game, player.as_ref(), *exclude_current)
        }
        HistoryPlan::Subject { event, filter } => subject_count_at(
            game,
            execution.ability,
            execution.context.controller,
            *event,
            filter,
            count,
        ),
        HistoryPlan::PlayedFiltered { filter, .. } => {
            if count != HistoryCount::Facts {
                return Err(unsupported("played history subject count"));
            }
            filtered_played(execution, game, filter)
        }
        HistoryPlan::Played(by) => {
            if count != HistoryCount::Facts {
                return Err(unsupported("played history subject count"));
            }
            match by {
                PlayedBy::Player(player) => played_count(execution, game, player, false),
                PlayedBy::Any => {
                    let own = played_count(execution, game, &PlayerRef::Controller {}, false)?.0;
                    let other = played_count(execution, game, &opponent(), false)?.0;
                    own.checked_add(other)
                        .map(Nat)
                        .ok_or_else(|| invalid("played-card count overflow"))
                }
            }
        }
        HistoryPlan::StatGain(_) | HistoryPlan::Lesson(_) => {
            Err(unsupported(if count == HistoryCount::Facts {
                "unfiltered current-turn played-card history"
            } else {
                "played history subject count"
            }))
        }
    }
}

pub(super) fn subject_count_at(
    game: &Game,
    source: Option<super::RuntimeAbilitySource>,
    controller: PlayerId,
    event: SubjectHistoryKind,
    filter: &SubjectFilter,
    count: HistoryCount,
) -> Result<Nat> {
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
        let kind = saved_kind(&fact.kind)?;
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
            keywords: &[],
            original_cost: None,
            kind: Some(kind),
            class: Some(&class),
            traits: &traits,
            names: &names,
        };
        let itself = source.is_some_and(|source| {
            source.holder.id.0 == fact.object && source.holder.generation.0 == fact.generation
        });
        if filter.matches_information(
            &information,
            itself,
            controller,
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

pub(super) fn sum(execution: &Execution, game: &Game, plan: &NumericHistoryPlan) -> Result<Int> {
    match plan {
        NumericHistoryPlan::LifeSum(filter) => {
            life_counter(execution, game, *filter, "leader_hp_increased").map(Int)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct DamageHistoryFact {
    recipient_link: Option<ObjectLink>,
    source_link: Option<ObjectLink>,
    recipient: (u64, u64),
    source: (u64, u64),
    controller: u32,
    traits: Vec<String>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Damage history shares committed game state."
)]
impl Game {
    pub(in crate::game::internal) fn internal_damage_source_link(
        &self,
        recipient: ObjectRef,
        holder: ObjectRef,
        controller: PlayerId,
        link: &SourceLink,
    ) -> bool {
        self.state.ir_damage_history.iter().any(|fact| {
            fact.recipient == (recipient.id.0, recipient.generation.0)
                && if link.source.itself == Some(true) {
                    fact.source == (holder.id.0, holder.generation.0)
                } else {
                    fact.controller == controller.seat
                        && link
                            .source
                            .trait_id
                            .as_ref()
                            .is_some_and(|id| fact.traits.contains(&id.0))
                }
        })
    }
}
