//! Keyword grants retain the provider and the receiving physical generation.
use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Ability, AbilityId, AbilityInstanceId, Action, CharacteristicPatch, DurationEnd, Effect,
    GrantedAbility, Instruction, InstructionId, KeywordId, Located, ObjectOperand, ObjectView,
    RuntimeAbilitySource, ScopeId, SourceSpan, TimeAnchor, printed_keywords,
};
use alloc::collections::BTreeSet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct KeywordState {
    generation: u64,
    grants: Vec<KeywordGrant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeywordGrant {
    end: EffectEnd,
    keywords: BTreeSet<String>,
    origin: GrantOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(in crate::game::internal) struct KeywordProvider {
    pub holder: String,
    pub generation: u64,
    pub card: String,
    pub span: SourceSpan,
    pub ability: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(in crate::game::internal) enum GrantOrigin {
    Installed {
        provider: KeywordProvider,
        execution: u64,
        instruction: u64,
    },
    Continuous {
        provider: KeywordProvider,
        effect: u64,
    },
}

impl GrantOrigin {
    pub(in crate::game::internal) fn drain_ability(&self) -> Located<Ability> {
        let provider = self.provider();
        Located {
            node: printed_keywords::drain_definition(
                AbilityId(provider.ability),
                ScopeId(provider.ability),
                InstructionId(1),
                &provider.span,
            ),
            source: provider.span.clone(),
        }
    }

    pub(in crate::game::internal) const fn provider(&self) -> &KeywordProvider {
        match self {
            Self::Installed { provider, .. } | Self::Continuous { provider, .. } => provider,
        }
    }
}

fn grants_drain(effect: &Effect) -> bool {
    let Effect::Continuous {
        patch: CharacteristicPatch::GrantAbility { abilities },
        ..
    } = effect
    else {
        return false;
    };
    abilities.iter().any(|ability| {
        matches!(
            ability,
            GrantedAbility::Keyword {
                keyword: KeywordId::Drain
            }
        )
    })
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only supported intrinsic keywords can be granted."
)]
pub(super) const fn keyword(keyword: KeywordId) -> Option<&'static str> {
    match keyword {
        KeywordId::Ward => Some("guard"),
        KeywordId::Aura => Some("aura"),
        KeywordId::Storm => Some("storm"),
        KeywordId::Rush => Some("rush"),
        KeywordId::Bane => Some("bane"),
        KeywordId::Drain => Some("drain"),
        KeywordId::Intimidate => Some("intimidate"),
        KeywordId::Assail => Some("assail"),
        _ => None,
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Grant {
        abilities,
        duration,
        provider,
        ..
    } = action
    else {
        return Ok(());
    };
    let supported_keywords = !abilities.is_empty() && abilities.iter().all(|ability| {
        matches!(ability, GrantedAbility::Keyword { keyword: id } if keyword(*id).is_some())
    });
    let current_provider = matches!(&provider.provider, ObjectView::Current { object }
        if object.as_ref() == &ObjectOperand::SourceHolder {});
    let fixed_duration = duration.anchor == (TimeAnchor::Installation {})
        && duration.usage.is_none()
        && duration.intervals.is_empty()
        && matches!(
            duration.end,
            DurationEnd::NoTemporalExpiry {} | DurationEnd::CurrentTurnEnd {}
        );
    if !supported_keywords || !current_provider || !fixed_duration {
        return Err(unsupported("fixed keyword grant"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword installation and expiry share generation-bound authority."
)]
impl Game {
    pub(in crate::game) fn internal_granted_keywords(
        &self,
        holder: &str,
    ) -> Result<BTreeSet<String>> {
        let object = self.object(holder)?;
        Ok(self
            .state
            .ir_keywords
            .get(holder)
            .filter(|state| object.zone == "field" && state.generation == object.generation)
            .into_iter()
            .flat_map(|state| &state.grants)
            .flat_map(|grant| grant.keywords.iter().cloned())
            .collect())
    }

    pub(super) fn keyword_provider(
        &self,
        holder: &str,
        ability: AbilityId,
    ) -> Result<KeywordProvider> {
        let generation = self.object(holder)?.generation;
        let (card, _) = self.internal_effective_print(holder)?;
        self.keyword_provider_at(holder, generation, card, ability)
    }

    fn keyword_provider_at(
        &self,
        holder: &str,
        generation: u64,
        card: String,
        ability: AbilityId,
    ) -> Result<KeywordProvider> {
        let span = self
            .card_runtime
            .definitions
            .cards
            .get(&card)
            .and_then(|program| {
                program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.node.id == ability)
            })
            .map(|ability| ability.source.clone())
            .ok_or_else(|| invalid("keyword grant provider definition"))?;
        Ok(KeywordProvider {
            holder: holder.into(),
            generation,
            card,
            span,
            ability: ability.0,
        })
    }

    pub(super) fn internal_has_drain_grants(&self) -> bool {
        self.state.ir_keywords.values().any(|state| {
            state
                .grants
                .iter()
                .any(|grant| grant.keywords.contains("drain"))
        }) || self.card_runtime.definitions.cards.values().any(|program| {
            program
                .card()
                .abilities
                .iter()
                .any(|ability| ability.node.declarations.iter().any(grants_drain))
        })
    }

    pub(super) fn internal_drain_grant(&self, holder: &str) -> Result<Option<GrantOrigin>> {
        let object = self.object(holder)?;
        if object.zone != "field" {
            return Ok(None);
        }
        if let Some(grant) = self
            .state
            .ir_keywords
            .get(holder)
            .filter(|state| state.generation == object.generation)
            .and_then(|state| {
                state
                    .grants
                    .iter()
                    .find(|grant| grant.keywords.contains("drain"))
            })
        {
            return Ok(Some(grant.origin.clone()));
        }
        Ok(self
            .internal_static_keyword_grants(holder)?
            .into_iter()
            .find_map(|(keyword, origin)| (keyword == KeywordId::Drain).then_some(origin)))
    }

    pub(super) fn internal_granted_drain_source(
        &mut self,
        holder: &str,
        origin: &GrantOrigin,
    ) -> Result<RuntimeAbilitySource> {
        let reference = self.internal_current_reference(holder)?;
        let key = (holder.into(), reference.generation.0, origin.clone());
        let instance = if let Some(instance) = self.card_runtime.keyword_instances.get(&key) {
            *instance
        } else {
            let next = self
                .card_runtime
                .next_instance
                .checked_add(1)
                .ok_or_else(|| invalid("ability instances exhausted"))?;
            let instance = AbilityInstanceId(next);
            self.card_runtime.next_instance = next;
            self.card_runtime.keyword_instances.insert(key, instance);
            instance
        };
        let provider = origin.provider();
        let source = RuntimeAbilitySource {
            holder: reference,
            instance,
            controller: super::player_id(&self.object(holder)?.controller)?,
            definition: AbilityId(provider.ability),
        };
        Ok(source)
    }

    fn installed_keyword_provider(&self, execution: &Execution) -> Result<KeywordProvider> {
        let authority = execution
            .ability
            .ok_or_else(|| invalid("keyword grant without ability source"))?;
        let provider_card = self
            .card_runtime
            .instances
            .iter()
            .find_map(|((_, _, card, _, _), instance)| {
                (*instance == authority.instance).then_some(card.clone())
            })
            .ok_or_else(|| invalid("keyword grant provider instance"))?;
        self.keyword_provider_at(
            &execution.context.source,
            authority.holder.generation.0,
            provider_card,
            authority.definition,
        )
    }

    pub(super) fn internal_grant(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let Action::Grant {
            subjects,
            abilities,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("keyword grant instruction"));
        };
        let end = EffectEnd::at(self, execution, &duration.end)?;
        let provider = self.installed_keyword_provider(execution)?;
        let origin = GrantOrigin::Installed {
            provider,
            execution: execution.id.0,
            instruction: instruction.id.0,
        };
        let keywords = abilities
            .iter()
            .map(|ability| {
                let GrantedAbility::Keyword { keyword: id } = ability else {
                    return Err(unsupported("keyword grant definition"));
                };
                keyword(*id)
                    .map(str::to_owned)
                    .ok_or_else(|| unsupported("unsupported keyword grant"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let mut installed = vec![];
        let mut seen = BTreeSet::new();
        for member in execution.objects(self, subjects)? {
            if !seen.insert((member.id, member.generation)) {
                continue;
            }
            let id = execution.object_id(member)?.to_owned();
            let object = self.object(&id)?;
            if object.generation != member.generation.0 || object.zone != "field" {
                continue;
            }
            let generation = object.generation;
            let state = self
                .state
                .ir_keywords
                .entry(id)
                .or_insert_with(|| KeywordState {
                    generation,
                    grants: vec![],
                });
            if state.generation != generation {
                *state = KeywordState {
                    generation,
                    grants: vec![],
                };
            }
            state.grants.push(KeywordGrant {
                end,
                keywords: keywords.clone(),
                origin: origin.clone(),
            });
            installed.push(member);
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Granted(installed));
        Ok(())
    }

    pub(in crate::game) fn internal_keywords_end_turn(&mut self, turn: u64) {
        self.state.ir_keywords.retain(|_, state| {
            state
                .grants
                .retain(|grant| grant.end != EffectEnd::Turn(turn));
            !state.grants.is_empty()
        });
    }
}
