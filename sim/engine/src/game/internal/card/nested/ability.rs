use super::{Context, Continuation, Execution, Game, Generation, Result, seat_name, unsupported};
use crate::game::internal::card::play::ability_cost;
use crate::game::internal::grant::EffectiveAbility;
use crate::game::internal::{AbilityOption, InputDomain, InstructionOutcome, Slot, Wait};
use crate::invalid;
use crate::ir::ability_play;
use crate::ir::{
    Ability, AbilityChoiceKind, AbilityCostPolicy, AbilityKind, AbilityOrigin,
    AbilityQualification, Action, Binding, Decision, Instruction, KeywordId, ObjectRef,
    RuntimeAbilityReference, RuntimeAbilitySource, TurnId,
};

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct AbilityWait {
    binding: Binding<RuntimeAbilityReference>,
    filter: AbilityChoiceKind,
    candidates: Vec<RuntimeAbilitySource>,
}

impl AbilityWait {
    pub(in crate::game::internal) fn validate(
        &self,
        game: &mut Game,
        execution: &Execution,
        handle: u64,
    ) -> Result<RuntimeAbilitySource> {
        let source = *self
            .candidates
            .get(usize::try_from(handle).map_err(invalid)?)
            .ok_or_else(|| invalid("foreign ability handle"))?;
        if game
            .current_ability(execution, source, self.filter)?
            .is_none()
        {
            return Err(invalid("ability choice is no longer effective"));
        }
        Ok(source)
    }

    pub(in crate::game::internal) fn bind(
        &self,
        execution: &mut Execution,
        source: Option<RuntimeAbilitySource>,
    ) {
        execution.slots.insert(
            self.binding.key(),
            Slot::Ability {
                source,
                filter: self.filter,
            },
        );
    }
}

pub(in crate::game::internal) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::PlayAbility { .. }) && ability_play::selection(action).is_none() {
        return Err(unsupported("effect ability play shape"));
    }
    Ok(())
}

pub(in crate::game::internal) fn check_choice(decision: &Decision) -> Result<()> {
    ability_play::choice(decision)
        .map(|_| ())
        .ok_or_else(|| unsupported("effective ability choice shape"))
}

fn matches(ability: &Ability, filter: AbilityChoiceKind) -> bool {
    match filter {
        AbilityChoiceKind::EvolutionTime => {
            ability.kind == AbilityKind::Triggered
                && matches!(
                    ability.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::OnEvolve,
                        ..
                    }
                )
        }
        AbilityChoiceKind::UnionBurst => {
            ability.qualification == AbilityQualification::UnionBurst
                && matches!(
                    ability.kind,
                    AbilityKind::Triggered | AbilityKind::Activated
                )
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Effect-selected abilities share effective sources and the ordinary formal play pipeline."
)]
impl Game {
    fn effective_ability_candidates(
        &mut self,
        execution: &Execution,
        holder: ObjectRef,
        filter: AbilityChoiceKind,
    ) -> Result<Vec<(RuntimeAbilitySource, EffectiveAbility)>> {
        let Some(physical) = execution.ids.get(&holder.id) else {
            return Ok(vec![]);
        };
        let Some(object) = self.state.objects.get(physical) else {
            return Ok(vec![]);
        };
        if object.generation != holder.generation.0
            || object.zone != "field"
            || object.controller != seat_name(execution.context.controller)?
        {
            return Ok(vec![]);
        }
        if filter == AbilityChoiceKind::UnionBurst && !self.internal_ub_enabled(physical)? {
            return Ok(vec![]);
        }
        let mut candidates = vec![];
        for ability in self.internal_effective_abilities(physical)? {
            if !matches(&ability.node, filter)
                || self.internal_ability_forbidden(physical, &ability.node)?
            {
                continue;
            }
            let source = self.internal_effective_source(physical, &ability)?;
            if filter == AbilityChoiceKind::EvolutionTime
                && let Some(limit) = ability.node.play.as_ref().and_then(|play| play.limit)
                && !self.card_runtime.quota_available(
                    source.instance,
                    TurnId(self.state.schedule.current()?),
                    limit,
                )
            {
                continue;
            }
            candidates.push((source, ability));
        }
        Ok(candidates)
    }

    fn current_ability(
        &mut self,
        execution: &Execution,
        source: RuntimeAbilitySource,
        filter: AbilityChoiceKind,
    ) -> Result<Option<EffectiveAbility>> {
        Ok(self
            .effective_ability_candidates(execution, source.holder, filter)?
            .into_iter()
            .find_map(|(current, definition)| (current == source).then_some(definition)))
    }

    pub(in crate::game::internal) fn internal_ability_choice(
        &mut self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let (holder, filter, binding) = ability_play::choice(decision)
            .ok_or_else(|| unsupported("effective ability choice"))?;
        let reference = execution.object(self, holder)?;
        let candidates = self.effective_ability_candidates(execution, reference, filter)?;
        let options = candidates
            .iter()
            .enumerate()
            .map(|(index, (_, ability))| {
                let source = &ability.definition.source;
                Ok(AbilityOption {
                    handle: u64::try_from(index).map_err(invalid)?,
                    source_card: source.card_no.clone(),
                    source_line: source.line,
                    source_section: source.section.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let wait = AbilityWait {
            binding,
            filter,
            candidates: candidates.into_iter().map(|(source, _)| source).collect(),
        };
        if wait.candidates.is_empty() {
            wait.bind(execution, None);
            return Ok(());
        }
        self.internal_wait(
            execution,
            decision.id,
            execution.context.controller,
            InputDomain::Abilities(options),
            Wait::Ability(wait),
        )
    }

    pub(in crate::game::internal) fn internal_nested_ability(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let (binding, policy) = ability_play::selection(&instruction.action)
            .ok_or_else(|| unsupported("effect ability play"))?;
        let Some(Slot::Ability { source, filter }) = execution.slots.get(&binding.key()) else {
            return Err(invalid("missing effective ability choice"));
        };
        let filter = *filter;
        if !matches!(
            (filter, policy),
            (AbilityChoiceKind::EvolutionTime, AbilityCostPolicy::Normal)
                | (
                    AbilityChoiceKind::UnionBurst,
                    AbilityCostPolicy::WaiveOriginal
                )
        ) {
            return Err(unsupported("ability choice cost policy"));
        }
        let Some(source) = *source else {
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::NestedPlay(None));
            return Ok(());
        };
        let Some(ability) = self.current_ability(execution, source, filter)? else {
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::NestedPlay(None));
            return Ok(());
        };
        let holder = execution
            .ids
            .get(&source.holder.id)
            .ok_or_else(|| invalid("missing selected ability holder"))?
            .clone();
        self.nested_attempt(
            execution,
            instruction.receipt,
            None,
            |game, continuation| {
                game.prepare_effect_ability(
                    &holder,
                    source,
                    ability.node.clone(),
                    policy,
                    continuation,
                )
            },
        )
    }

    fn prepare_effect_ability(
        &mut self,
        holder: &str,
        source: RuntimeAbilitySource,
        ability: Ability,
        policy: AbilityCostPolicy,
        continuation: Box<Continuation>,
    ) -> Result<Execution> {
        self.check_internal_resources()?;
        let mut execution = self.fresh_card_execution(Context {
            source: holder.into(),
            generation: Generation(source.holder.generation.0),
            controller: source.controller,
        })?;
        execution.nested = Some(continuation);
        execution.bind_source(self, source)?;
        if let Some(play) = &ability.play {
            if policy == AbilityCostPolicy::WaiveOriginal {
                ability_cost::seed_original_x(&mut execution, &play.parameters)?;
            }
            self.internal_use_prerequisite(&execution, &play.costs)?;
            execution.captures(self, &play.captures)?;
        }
        self.prepare_ability_play(&mut execution, ability, policy)?;
        Ok(execution)
    }
}
