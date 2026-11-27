use super::source::SourceSnapshot;
use super::{
    Context, Execution, Fact, Game, InputDomain, InstructionOutcome, Result, Wait, invalid,
    player_id, seat_name, unsupported,
};
use crate::game::damage::DamageReplacement;
use crate::game::player::ReplacementChoice;
use crate::game::player::Viewer;
use crate::game::{BTreeMap, Frame, int, string};
use crate::ir::{
    Action, DamageEvent, DamageFlags, DamageQuantityOutcome, DecisionId, Generation, Instruction,
    Int, Nat, PlayerId, PositiveInt, ReceiptId, RuntimeDamageRecipient,
};
use alloc::collections::BTreeSet;
use core::num::NonZeroU64;
use serde_json::{Value, json};

/// One public effect occurrence in a replacement-order question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementOption {
    /// Handle valid only for this question.
    pub handle: u64,
    /// Public printed identity of its provider.
    pub card: String,
    /// Public provider name.
    pub name: String,
    /// Public printed line, when the effect has one.
    pub line: Option<u64>,
}

#[derive(Clone, Debug)]
pub(in crate::game) struct DamageOccurrence {
    pub(in crate::game) event: DamageEvent,
    pub(super) source: SourceSnapshot,
}

impl DamageOccurrence {
    pub(in crate::game) const fn source_generation(&self) -> u64 {
        self.source.view().object.generation.0
    }
}

#[derive(Clone, Debug)]
struct AbilityDamage {
    receipt: ReceiptId,
    source: SourceSnapshot,
    controller: PlayerId,
    requested: i64,
}

#[derive(Clone, Debug)]
enum DamageOrigin {
    Ability(Box<AbilityDamage>),
    Battle(Box<Frame>),
}

#[derive(Clone, Debug)]
pub(super) struct DamageRun {
    origin: DamageOrigin,
    remaining: Vec<(RuntimeDamageRecipient, Value)>,
    hits: Vec<Value>,
    consumed: BTreeMap<u64, i64>,
    facts: Vec<DamageOccurrence>,
    choices: Vec<DamageReplacement>,
    choice_links: BTreeMap<u64, super::observation::ObjectLink>,
    choice_origins: Vec<Option<super::observation::EffectOrigin>>,
}

impl DamageRun {
    const fn new(origin: DamageOrigin, remaining: Vec<(RuntimeDamageRecipient, Value)>) -> Self {
        Self {
            origin,
            remaining,
            hits: vec![],
            consumed: BTreeMap::new(),
            facts: vec![],
            choices: vec![],
            choice_links: BTreeMap::new(),
            choice_origins: vec![],
        }
    }

    pub(in crate::game::internal) fn player_options(
        &self,
        game: &Game,
        actor: Viewer,
        options: &[ReplacementOption],
    ) -> Result<Vec<ReplacementChoice>> {
        if options.len() != self.choices.len() || options.len() != self.choice_origins.len() {
            return Err(invalid("inconsistent replacement choices"));
        }
        Ok(options
            .iter()
            .zip(&self.choice_origins)
            .map(|(option, origin)| ReplacementChoice {
                handle: option.handle,
                card: option.card.clone(),
                name: option.name.clone(),
                line: option.line,
                object: self
                    .choice_links
                    .get(&option.handle)
                    .and_then(|link| link.observed(game, actor)),
                source: origin
                    .as_ref()
                    .and_then(|origin| origin.observed(game, actor)),
            })
            .collect())
    }

    pub(super) fn validate(&self, handles: &[u64]) -> Result<Vec<DamageReplacement>> {
        let mut seen = BTreeSet::new();
        if handles.len() != self.choices.len() {
            return Err(invalid("incomplete damage replacement order"));
        }
        handles
            .iter()
            .map(|handle| {
                if !seen.insert(*handle) {
                    return Err(invalid("duplicate damage replacement handle"));
                }
                let index = usize::try_from(*handle).map_err(invalid)?;
                self.choices
                    .get(index)
                    .cloned()
                    .ok_or_else(|| invalid("foreign damage replacement handle"))
            })
            .collect()
    }
}

fn order_independent(replacements: &[DamageReplacement]) -> bool {
    if replacements.len() > 1
        && replacements
            .iter()
            .any(|replacement| replacement.consumption.is_some())
    {
        return false;
    }
    replacements.len() <= 1
        || replacements.iter().any(|effect| effect.prevent)
        || replacements
            .first()
            .and_then(|effect| effect.cap)
            .is_some_and(|cap| replacements.iter().all(|effect| effect.cap == Some(cap)))
        || replacements.iter().all(|effect| effect.multiply.is_some())
        || replacements.iter().all(|effect| {
            !effect.prevent
                && effect.cap.is_none()
                && effect.multiply.is_none()
                && effect.delta >= 0
        })
        || replacements.iter().all(|effect| {
            !effect.prevent
                && effect.cap.is_none()
                && effect.multiply.is_none()
                && effect.delta <= 0
        })
}

impl Game {
    pub(super) fn internal_damage(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Damage {
            recipients,
            amount: proposed,
            source,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("damage instruction"));
        };
        let (shares, requested) = execution.damage_shares(self, recipients, proposed)?;
        let source = execution.damage_source(self, source)?;
        let source_id = execution.object_id(source.view().object)?.to_owned();
        let controller = execution.player(self, &instruction.source.controller)?;
        let mut remaining = Vec::new();
        let mut non_active = Vec::new();
        let active = player_id(self.active())?;
        for share in shares {
            let target = match share.recipient {
                RuntimeDamageRecipient::Leader { player } => {
                    format!("{}.leader", seat_name(player)?)
                }
                RuntimeDamageRecipient::Object { object } => {
                    let id = execution.object_id(object)?;
                    let current = self.object(id)?;
                    if current.generation != object.generation.0 {
                        continue;
                    }
                    if current.zone != "field" {
                        return Err(unsupported("non-field damage recipient"));
                    }
                    if !self.object_type(id)?.contains("フォロワー") {
                        return Err(unsupported("non-follower damage recipient"));
                    }
                    id.to_owned()
                }
            };
            let amount = i64::try_from(share.amount.0).map_err(invalid)?;
            if amount > 0 {
                let affected = match share.recipient {
                    RuntimeDamageRecipient::Leader { player } => player,
                    RuntimeDamageRecipient::Object { .. } => {
                        player_id(&self.object(&target)?.controller)?
                    }
                };
                let hit = (
                    share.recipient,
                    json!({"controller":seat_name(controller)?,"source":source_id,"source_generation":source.view().object.generation.0,"target":target,"amount":amount,"battle":false,"attack":false}),
                );
                if affected == active {
                    remaining.push(hit);
                } else {
                    non_active.push(hit);
                }
            }
        }
        // Simultaneous decisions belong to the active player first (CR 1.3.4).
        remaining.extend(non_active);
        remaining.reverse();
        self.continue_damage(
            execution,
            DamageRun::new(
                DamageOrigin::Ability(Box::new(AbilityDamage {
                    receipt: instruction.receipt,
                    source,
                    controller,
                    requested,
                })),
                remaining,
            ),
            None,
        )
    }

    pub(in crate::game) fn internal_battle_damage(
        &mut self,
        frame: Frame,
        hits: Vec<Value>,
    ) -> Result<()> {
        let mut execution = self.fresh_card_execution(Context {
            source: frame.source.clone(),
            generation: Generation(self.object(&frame.source)?.generation),
            controller: player_id(&frame.controller)?,
        })?;
        let remaining = hits
            .into_iter()
            .filter(|hit| int(&hit["amount"]) > 0)
            .map(|hit| {
                let target = string(&hit["target"]);
                let recipient = if let Some(seat) = target.strip_suffix(".leader") {
                    RuntimeDamageRecipient::Leader {
                        player: player_id(seat)?,
                    }
                } else {
                    RuntimeDamageRecipient::Object {
                        object: execution.reference(self, target)?,
                    }
                };
                Ok((recipient, hit))
            })
            .collect::<Result<_>>()?;
        // The counter-hit is popped first, so the turn player orders their replacements first.
        let run = DamageRun::new(DamageOrigin::Battle(Box::new(frame)), remaining);
        self.continue_damage(&mut execution, run, None)?;
        if execution.waiting.is_some() {
            self.typed = Some(execution);
        }
        Ok(())
    }

    fn damage_replacement_options(
        replacements: &[DamageReplacement],
    ) -> Result<Vec<ReplacementOption>> {
        replacements
            .iter()
            .zip(0_u64..)
            .map(|(replacement, handle)| {
                Ok(ReplacementOption {
                    handle,
                    card: replacement.label.card.clone(),
                    name: replacement.label.name.clone(),
                    line: replacement.label.line,
                })
            })
            .collect::<Result<Vec<_>>>()
    }

    pub(super) fn continue_damage(
        &mut self,
        execution: &mut Execution,
        mut run: DamageRun,
        order: Option<Vec<DamageReplacement>>,
    ) -> Result<()> {
        let ability = match &run.origin {
            DamageOrigin::Ability(source) => Some(source.as_ref()),
            DamageOrigin::Battle(_) => None,
        };
        let mut selected = order;
        while let Some((recipient, mut hit)) = run.remaining.pop() {
            let replacements = if let Some(chosen) = selected.take() {
                chosen
            } else {
                self.damage_replacements_with_source(
                    &hit,
                    &run.consumed,
                    ability.map(|ability| &ability.source),
                )?
            };
            if !order_independent(&replacements) && run.choices.is_empty() {
                let actor = match recipient {
                    RuntimeDamageRecipient::Leader { player } => player,
                    RuntimeDamageRecipient::Object { object } => {
                        player_id(&self.object(execution.object_id(object)?)?.controller)?
                    }
                };
                let options = Self::damage_replacement_options(&replacements)?;
                run.remaining.push((recipient, hit));
                run.choice_origins = replacements
                    .iter()
                    .map(|choice| self.player_replacement_origin(&choice.reference))
                    .collect::<Result<_>>()?;
                run.choice_links = replacements
                    .iter()
                    .zip(0_u64..)
                    .filter_map(|(choice, handle)| {
                        self.player_replacement_link(&choice.reference)
                            .map(|link| (handle, link))
                    })
                    .collect();
                run.choices = replacements;
                self.internal_wait(
                    execution,
                    DecisionId(0),
                    actor,
                    InputDomain::DamageReplacements(options),
                    Wait::DamageReplacements(Box::new(run)),
                )?;
                return Ok(());
            }
            run.choices.clear();
            let ordered = if replacements
                .iter()
                .any(|effect| effect.consumption.is_some())
            {
                replacements
            } else {
                replacements
                    .iter()
                    .find(|effect| effect.prevent)
                    .cloned()
                    .map_or(replacements, |prevention| vec![prevention])
            };
            let (actual, uses) = DamageReplacement::fold_order(int(&hit["amount"]), ordered)?;
            for usage in uses {
                let count = run.consumed.entry(usage).or_default();
                *count = count.saturating_add(1);
            }
            if actual == 0 {
                continue;
            }
            self.internal_damage_arithmetic(string(&hit["target"]), actual)?;
            hit["amount"] = json!(actual);
            run.hits.push(hit);
            if let Some(ability) = ability {
                run.facts.push(DamageOccurrence {
                    source: ability.source.clone(),
                    event: DamageEvent {
                        source: ability.source.view(),
                        recipient,
                        amount: PositiveInt(
                            NonZeroU64::new(u64::try_from(actual).map_err(invalid)?)
                                .ok_or_else(|| invalid("positive actual damage"))?,
                        ),
                        flags: DamageFlags::Ability,
                        controller: ability.controller,
                    },
                });
            }
        }
        self.finish_damage(execution, run)
    }

    fn finish_damage(&mut self, execution: &mut Execution, mut run: DamageRun) -> Result<()> {
        match run.origin {
            DamageOrigin::Battle(frame) => {
                // Publish the attack hit first in the simultaneous damage batch.
                run.hits.reverse();
                self.commit_damage_uses(&run.consumed)?;
                self.apply_damage(&run.hits, &frame, None)
            }
            DamageOrigin::Ability(ability) => {
                let actual = run.facts.iter().try_fold(0_u64, |sum, fact| {
                    sum.checked_add(fact.event.amount.0.get())
                        .ok_or_else(|| invalid("damage outcome overflow"))
                })?;
                self.commit_damage_uses(&run.consumed)?;
                self.apply_damage(&run.hits, &execution.frame(), Some(&run.facts))?;
                for fact in run.facts {
                    execution.record_fact(self, Fact::Damage(fact.event));
                }
                execution.receipts.insert(
                    ability.receipt,
                    InstructionOutcome::Damage(DamageQuantityOutcome {
                        requested: Int(ability.requested),
                        actual: Nat(actual),
                    }),
                );
                Ok(())
            }
        }
    }
}
