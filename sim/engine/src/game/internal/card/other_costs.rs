use super::super::{Context, Execution, InstructionOutcome, player_id};
use super::{Ability, Game, Located, Result, invalid, unsupported};
use crate::game::internal::duration::EffectEnd;
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Action, CostChange, CostKind, Duration,
    DurationEnd, Effect, EffectReference, Expr, Generation, InstallationSubjects, InstallationTime,
    Instruction, Int, Nat, ObjectOperand, PlayerId, PlayerRef, PlayerRelation, SelectionView,
    Selector, TimeAnchor, UsageScope, ZoneKind,
};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct InstalledCost {
    id: u64,
    controller: PlayerId,
    players: Vec<PlayerId>,
    subjects: Selector,
    change: CostChange,
    end: EffectEnd,
    limited: bool,
}

pub(super) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| matches!(effect, Effect::CostModifier { .. }))
}

fn check_change(change: &CostChange) -> Result<()> {
    match change {
        CostChange::Delta {
            amount: Expr::Literal(_),
        } => Ok(()),
        CostChange::Set {
            amount: Expr::Literal(Int(amount)),
        } if *amount >= 0 => Ok(()),
        _ => Err(unsupported("fixed player cost change")),
    }
}

fn check_subjects(subjects: &Selector) -> Result<()> {
    match subjects {
        Selector::Union { sets } if !sets.is_empty() => {
            for set in sets {
                check_subjects(set)?;
            }
            Ok(())
        }
        Selector::Zone {
            players,
            zone: ZoneKind::Hand | ZoneKind::Ex,
            view: SelectionView::Current {},
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            super::selection::check_predicate(predicate, *binding, false)
        }
        _ => Err(unsupported("player cost filter")),
    }
}

fn check_duration(duration: &Duration, installed: bool) -> Result<()> {
    if duration.anchor != (TimeAnchor::Installation {}) || !duration.intervals.is_empty() {
        return Err(unsupported("player cost lifetime"));
    }
    if let Some(limit) = &duration.usage
        && (!installed
            || limit.count != Nat(1)
            || limit.scope
                != (UsageScope::SharedPlayer {
                    player: PlayerRef::Controller {},
                    effect: EffectReference::CurrentInstallation {},
                }))
    {
        return Err(unsupported("player cost quota"));
    }
    match &duration.end {
        DurationEnd::NoTemporalExpiry {} => Ok(()),
        DurationEnd::CurrentTurnEnd {} if installed => Ok(()),
        DurationEnd::While { predicate } if !installed => {
            super::costs::check_condition(predicate, false)
        }
        _ => Err(unsupported("player cost lifetime")),
    }
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !is_static(ability)
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static player cost header"));
    }
    for effect in &ability.declarations {
        let Effect::CostModifier {
            kind: CostKind::Play,
            subjects,
            change,
            duration,
            usage: None,
        } = effect
        else {
            return Err(unsupported("static player cost declaration"));
        };
        check_subjects(subjects)?;
        check_change(change)?;
        check_duration(duration, false)?;
    }
    Ok(())
}

pub(in crate::game::internal) fn check_install(action: &Action) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("player cost installation"));
    };
    let Effect::CostModifier {
        kind: CostKind::Play,
        subjects,
        change,
        duration: inner,
        usage: None,
    } = effect.as_ref()
    else {
        return Err(unsupported("player cost effect"));
    };
    if players.relative_to != (PlayerRef::Controller {}) || inner != duration {
        return Err(unsupported("player cost context"));
    }
    // One shared quota per installation would be spent by whichever player plays first.
    if duration.usage.is_some()
        && !matches!(
            players.relation,
            PlayerRelation::SelfPlayer | PlayerRelation::Opponents
        )
    {
        return Err(unsupported("player cost quota across players"));
    }
    check_subjects(subjects)?;
    check_change(change)?;
    check_duration(duration, true)
}

fn matches(
    game: &Game,
    execution: &Execution,
    subjects: &Selector,
    holder: &str,
    origin: ZoneKind,
) -> Result<bool> {
    match subjects {
        Selector::Union { sets } => {
            for set in sets {
                if matches(game, execution, set, holder, origin)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Selector::Zone {
            players,
            zone,
            predicate,
            ..
        } => Ok(*zone == origin
            && execution
                .players(game, players)?
                .contains(&player_id(&game.object(holder)?.controller)?)
            && super::selection::matches_candidate(game, holder, predicate)?),
        _ => Err(unsupported("player cost selector")),
    }
}

fn apply(change: &CostChange, base: &mut i64, delta: &mut i64) -> Result<()> {
    match change {
        CostChange::Delta {
            amount: Expr::Literal(Int(amount)),
        } => {
            *delta = delta
                .checked_add(*amount)
                .ok_or_else(|| invalid("play cost overflow"))?;
        }
        CostChange::Set {
            amount: Expr::Literal(Int(amount)),
        } => *base = *amount,
        _ => return Err(unsupported("player cost change")),
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "All card payment paths share the installed cost layer."
)]
impl Game {
    pub(in crate::game::internal) fn internal_install_player_cost(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action)?;
        let Action::InstallEffect {
            subjects: InstallationSubjects::Players { players, .. },
            effect,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("player cost installation"));
        };
        let Effect::CostModifier {
            subjects, change, ..
        } = effect.as_ref()
        else {
            return Err(unsupported("player cost effect"));
        };
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let players = execution.players(self, players)?;
        let id = self.state.next_ir_effect;
        self.state.next_ir_effect = id
            .checked_add(1)
            .ok_or_else(|| invalid("cost identities exhausted"))?;
        self.card_runtime.other_costs.push(InstalledCost {
            id,
            controller: execution.context.controller,
            players,
            subjects: subjects.clone(),
            change: change.clone(),
            end,
            limited: duration.usage.is_some(),
        });
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }
    pub(in crate::game::internal) fn internal_matching_player_costs(
        &self,
        execution: &Execution,
    ) -> Result<Vec<u64>> {
        let mut ids = Vec::new();
        let holder = &execution.context.source;
        let origin = execution
            .play_origin
            .ok_or_else(|| invalid("missing play origin"))?;
        for cost in &self.card_runtime.other_costs {
            if matches!(cost.end, EffectEnd::Turn(turn) if self.state.schedule.current()? != turn)
                || !cost.players.contains(&execution.context.controller)
            {
                continue;
            }
            let mut provider = execution.clone();
            provider.context.controller = cost.controller;
            if matches(self, &provider, &cost.subjects, holder, origin)? {
                ids.push(cost.id);
            }
        }
        Ok(ids)
    }
    pub(in crate::game::internal) fn internal_player_cost_end_turn(&mut self, turn: u64) {
        self.card_runtime
            .other_costs
            .retain(|cost| cost.end != EffectEnd::Turn(turn));
    }
    pub(in crate::game::internal) fn internal_consume_player_costs(&mut self, ids: &[u64]) {
        self.card_runtime
            .other_costs
            .retain(|cost| !cost.limited || !ids.contains(&cost.id));
    }
    pub(in crate::game::internal) fn internal_other_play_cost(
        &self,
        execution: &Execution,
        mut base: i64,
        mut delta: i64,
    ) -> Result<i64> {
        let holder = &execution.context.source;
        (base, delta) = self.internal_installed_play_parts(holder, base, delta)?;
        let origin = execution
            .play_origin
            .ok_or_else(|| invalid("missing play origin"))?;
        for source in self.field_ids() {
            let (card, _) = self.internal_effective_print(&source)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let abilities = self
                .internal_printed_abilities(&source)?
                .into_iter()
                .filter(|ability| is_static(&ability.node))
                .collect::<Vec<_>>();
            if abilities.is_empty() {
                continue;
            }
            let object = self.object(&source)?;
            let provider = self.card_execution(
                Context {
                    source: source.clone(),
                    generation: Generation(object.generation),
                    controller: player_id(&object.controller)?,
                },
                execution.ids.clone(),
            )?;
            for ability in abilities {
                for effect in &ability.node.declarations {
                    let Effect::CostModifier {
                        subjects,
                        change,
                        duration,
                        ..
                    } = effect
                    else {
                        return Err(unsupported("static cost declaration"));
                    };
                    if let DurationEnd::While { predicate } = &duration.end
                        && !provider.boolean(self, predicate)?.0
                    {
                        continue;
                    }
                    if matches(self, &provider, subjects, holder, origin)? {
                        apply(change, &mut base, &mut delta)?;
                    }
                }
            }
        }
        let matching = self.internal_matching_player_costs(execution)?;
        for cost in &self.card_runtime.other_costs {
            if matching.contains(&cost.id) {
                apply(&cost.change, &mut base, &mut delta)?;
            }
        }
        base.checked_add(delta)
            .ok_or_else(|| invalid("play cost overflow"))
    }
}
