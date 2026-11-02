//! Ability grants retain the text provider and receiving physical generation.
use super::duration::EffectEnd;
use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Ability, AbilityId, AbilityInstanceId, AbilityKind, AbilityOrigin, AbilityQualification,
    Action, CharacteristicPatch, DurationEnd, Effect, EventPattern, GrantedAbility, Instruction,
    InstructionId, KeywordId, Located, ObjectOperand, ObjectView, PlayerRef, RuntimeAbilitySource,
    ScopeId, SourceSpan, TimeAnchor, TriggerCondition, ZoneKind, granted_cost, printed_keywords,
};
use alloc::collections::BTreeSet;
use core::ops::Deref;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    definitions: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct EffectiveAbility {
    pub definition: Located<Ability>,
    pub grant: Option<GrantOrigin>,
}

impl From<Located<Ability>> for EffectiveAbility {
    fn from(definition: Located<Ability>) -> Self {
        Self {
            definition,
            grant: None,
        }
    }
}

impl Deref for EffectiveAbility {
    type Target = Located<Ability>;
    fn deref(&self) -> &Self::Target {
        &self.definition
    }
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
        through: Option<Box<Self>>,
    },
}

impl GrantOrigin {
    pub(in crate::game::internal) fn public_stamp(&self) -> Value {
        match self {
            Self::Installed {
                execution,
                instruction,
                ..
            } => json!({"execution":execution,"instruction":instruction}),
            // Continuous providers are current public field objects; a departed installer stays opaque.
            Self::Continuous {
                provider,
                effect,
                through,
            } => json!({
                "provider":provider.holder,"generation":provider.generation,"effect":effect,
                "through":through.as_ref().map(|origin| origin.public_stamp()),
            }),
        }
    }

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
    let supported_keywords = !abilities.is_empty()
        && abilities.iter().all(|ability| match ability {
            GrantedAbility::Keyword { keyword: id } => keyword(*id).is_some(),
            GrantedAbility::Definition { ability } => supported_definition(ability),
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
    if !supported_keywords {
        return Err(unsupported("granted definition event or keyword"));
    }
    if !current_provider || !fixed_duration {
        return Err(unsupported("fixed keyword grant"));
    }
    Ok(())
}

pub(super) fn supported_definition(ability: &Ability) -> bool {
    if ability.kind == AbilityKind::Static {
        return ability.active_zones == [ZoneKind::Field] || static_cost(ability);
    }
    // The closed trigger grammar supplies the nested entry's private-read proof.
    ability.kind == AbilityKind::Triggered
        && ability.qualification == AbilityQualification::Ordinary
        && (ability.active_zones == [ZoneKind::Field] || super::card::triggers::ex_phase(ability))
        && matches!(
            ability.trigger.as_ref().map(|trigger| &trigger.on),
            Some(TriggerCondition::Event {
                pattern: EventPattern::Attack { .. }
                    | EventPattern::Phase { .. }
                    | EventPattern::LeaveField { .. }
            })
        )
}

// The shared delta helper checks effect shape; this additionally closes the ability header.
pub(super) fn static_cost(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && ability.active_zones == [ZoneKind::Ex, ZoneKind::Resolution]
        && ability.qualification == AbilityQualification::Ordinary
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability.source.holder == (ObjectOperand::SourceHolder {})
        && ability.source.controller == (PlayerRef::Controller {})
        && ability.event_captures.is_empty()
        && ability.play.is_none()
        && ability.trigger.is_none()
        && ability.body.is_none()
        && matches!(ability.declarations.as_slice(), [effect] if granted_cost::delta(effect).is_some())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword installation and expiry share generation-bound authority."
)]
impl Game {
    pub(in crate::game::internal) fn internal_has_granted_definitions(&self) -> bool {
        self.state.ir_keywords.values().any(|state| state.grants.iter().any(|grant| !grant.definitions.is_empty()))
            || self.card_runtime.definitions.cards.values().any(|program| program.card().abilities.iter().any(|ability| ability.node.declarations.iter().any(|effect| {
                matches!(effect, Effect::Continuous { patch: CharacteristicPatch::GrantAbility { abilities }, .. } if abilities.iter().any(|granted| matches!(granted, GrantedAbility::Definition { .. })))
            })))
    }

    pub(in crate::game::internal) fn internal_has_local_definitions(&self, holder: &str) -> bool {
        self.state.ir_keywords.get(holder).is_some_and(|state| {
            self.state
                .objects
                .get(holder)
                .is_some_and(|object| object.generation == state.generation)
                && state
                    .grants
                    .iter()
                    .any(|grant| !grant.definitions.is_empty())
        })
    }

    pub(in crate::game::internal) fn internal_local_abilities(
        &self,
        holder: &str,
    ) -> Result<Vec<EffectiveAbility>> {
        let mut result = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .map(|definition| EffectiveAbility {
                definition,
                grant: None,
            })
            .collect::<Vec<_>>();
        let object = self.object(holder)?;
        let zone = match object.zone.as_str() {
            "field" => ZoneKind::Field,
            "ex" => ZoneKind::Ex,
            "resolution" => ZoneKind::Resolution,
            _ => return Ok(result),
        };
        if let Some(state) = self
            .state
            .ir_keywords
            .get(holder)
            .filter(|state| state.generation == object.generation)
        {
            for grant in &state.grants {
                for definition in &grant.definitions {
                    let ability = self
                        .card_runtime
                        .granted_definitions
                        .get(&(grant.origin.provider().card.clone(), AbilityId(*definition)))
                        .ok_or_else(|| invalid("missing installed definition"))?;
                    if !ability.active_zones.contains(&zone) {
                        continue;
                    }
                    result.push(EffectiveAbility {
                        definition: Located {
                            node: ability.clone(),
                            source: grant.origin.provider().span.clone(),
                        },
                        grant: Some(grant.origin.clone()),
                    });
                }
            }
        }
        Ok(result)
    }

    pub(in crate::game::internal) fn internal_effective_abilities(
        &self,
        holder: &str,
    ) -> Result<Vec<EffectiveAbility>> {
        let mut result = self.internal_local_abilities(holder)?;
        for (ability, origin) in self.internal_static_grants(holder)? {
            if let GrantedAbility::Definition { ability } = ability {
                result.push(EffectiveAbility {
                    definition: Located {
                        node: *ability,
                        source: origin.provider().span.clone(),
                    },
                    grant: Some(origin),
                });
            }
        }
        Ok(result)
    }

    pub(in crate::game::internal) fn internal_effective_source(
        &mut self,
        holder: &str,
        ability: &EffectiveAbility,
    ) -> Result<RuntimeAbilitySource> {
        match &ability.grant {
            Some(origin) => self.internal_definition_source(holder, origin, ability.node.id),
            None => self.internal_card_source(
                holder,
                ability.node.id,
                super::player_id(&self.object(holder)?.controller)?,
            ),
        }
    }

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
        }) || self
            .card_runtime
            .granted_definitions
            .values()
            .any(|ability| ability.declarations.iter().any(grants_drain))
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
        self.internal_definition_source(holder, origin, AbilityId(origin.provider().ability))
    }

    fn internal_definition_source(
        &mut self,
        holder: &str,
        origin: &GrantOrigin,
        definition: AbilityId,
    ) -> Result<RuntimeAbilitySource> {
        let reference = self.internal_current_reference(holder)?;
        let key = (
            holder.into(),
            reference.generation.0,
            origin.clone(),
            definition,
        );
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
        let source = RuntimeAbilitySource {
            holder: reference,
            instance,
            controller: super::player_id(&self.object(holder)?.controller)?,
            definition,
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
            .filter_map(|ability| {
                let GrantedAbility::Keyword { keyword: id } = ability else {
                    return None;
                };
                Some(
                    keyword(*id)
                        .map(str::to_owned)
                        .ok_or_else(|| unsupported("unsupported keyword grant")),
                )
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let mut definitions = vec![];
        for ability in abilities {
            if let GrantedAbility::Definition { ability } = ability {
                definitions.push(ability.id.0);
                self.card_runtime.granted_definitions.insert(
                    (origin.provider().card.clone(), ability.id),
                    *ability.clone(),
                );
            }
        }
        let mut installed = vec![];
        let mut seen = BTreeSet::new();
        for member in execution.objects(self, subjects)? {
            if !seen.insert((member.id, member.generation)) {
                continue;
            }
            let id = execution.object_id(member)?.to_owned();
            let object = self.object(&id)?;
            if object.generation != member.generation.0 {
                continue;
            }
            if !matches!(object.zone.as_str(), "field" | "ex") {
                if !definitions.is_empty() {
                    return Err(unsupported("definition grant requires Field or EX"));
                }
                continue;
            }
            if !definitions.is_empty() {
                // A granted definition needs the receiver's ordinary ability authority too.
                self.internal_printed_abilities(&id)?;
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
                definitions: definitions.clone(),
            });
            installed.push(member);
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Granted(installed));
        Ok(())
    }

    pub(in crate::game::internal) fn internal_grants_rule_carry(
        &mut self,
        holder: &str,
        before: u64,
    ) -> Result<()> {
        let generation = self.object(holder)?.generation;
        if let Some(state) = self
            .state
            .ir_keywords
            .get_mut(holder)
            .filter(|state| state.generation == before)
        {
            state.generation = generation;
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_static_origin(
        &self,
        holder: &str,
        located: &EffectiveAbility,
        effect: u64,
    ) -> Result<GrantOrigin> {
        let provider = if let Some(origin) = &located.grant {
            KeywordProvider {
                holder: holder.into(),
                generation: self.object(holder)?.generation,
                card: origin.provider().card.clone(),
                span: located.source.clone(),
                ability: located.node.id.0,
            }
        } else {
            self.keyword_provider(holder, located.node.id)?
        };
        Ok(GrantOrigin::Continuous {
            provider,
            effect,
            through: located.grant.clone().map(Box::new),
        })
    }

    pub(in crate::game::internal) fn internal_granted_cost_active(
        &self,
        holder: Option<&str>,
    ) -> bool {
        self.state.ir_keywords.iter().any(|(id, state)| {
            holder.is_none_or(|holder| holder == id)
                && self.state.objects.get(id).is_some_and(|object| {
                    state.generation == object.generation
                        && matches!(object.zone.as_str(), "ex" | "resolution")
                })
                && state.grants.iter().any(|grant| {
                    grant.definitions.iter().any(|definition| {
                        self.card_runtime
                            .granted_definitions
                            .get(&(grant.origin.provider().card.clone(), AbilityId(*definition)))
                            .is_some_and(static_cost)
                    })
                })
        })
    }

    pub(in crate::game::internal) fn internal_granted_play_delta(
        &self,
        holder: &str,
    ) -> Result<i64> {
        let object = self.object(holder)?;
        if !matches!(object.zone.as_str(), "ex" | "resolution") {
            return Ok(0);
        }
        let mut delta = 0_i64;
        if let Some(state) = self
            .state
            .ir_keywords
            .get(holder)
            .filter(|state| state.generation == object.generation)
        {
            for grant in &state.grants {
                for id in &grant.definitions {
                    let ability = self
                        .card_runtime
                        .granted_definitions
                        .get(&(grant.origin.provider().card.clone(), AbilityId(*id)))
                        .ok_or_else(|| invalid("missing granted cost definition"))?;
                    if !static_cost(ability) {
                        continue;
                    }
                    for effect in &ability.declarations {
                        if let Some(amount) = granted_cost::delta(effect) {
                            delta = delta
                                .checked_add(amount)
                                .ok_or_else(|| invalid("granted play cost overflow"))?;
                        }
                    }
                }
            }
        }
        Ok(delta)
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
