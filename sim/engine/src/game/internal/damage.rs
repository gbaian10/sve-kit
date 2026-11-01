use super::{
    Execution, Fact, Game, InputDomain, InstructionOutcome, Result, Wait, invalid, player_id,
    seat_name, unsupported,
};
use crate::game::damage::DamageReplacement;
use crate::game::{BTreeMap, int, string};
use crate::ir::{
    Action, DamageEvent, DamageFlags, DamageQuantityOutcome, DecisionId, Instruction, Int, Nat,
    PlayerId, PositiveInt, RuntimeDamageRecipient, RuntimeObjectView,
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
pub(super) struct DamageRun {
    instruction: Box<Instruction>,
    source: RuntimeObjectView,
    controller: PlayerId,
    requested: i64,
    remaining: Vec<(RuntimeDamageRecipient, Value)>,
    hits: Vec<Value>,
    facts: Vec<DamageEvent>,
    choices: Vec<DamageReplacement>,
}

impl DamageRun {
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
    replacements.len() <= 1
        || replacements.iter().any(|effect| effect.prevent)
        || replacements
            .first()
            .and_then(|effect| effect.cap)
            .is_some_and(|cap| replacements.iter().all(|effect| effect.cap == Some(cap)))
        || replacements.iter().all(|effect| {
            !effect.prevent && effect.cap.is_none() && effect.set.is_none() && effect.delta >= 0
        })
        || replacements.iter().all(|effect| {
            !effect.prevent && effect.cap.is_none() && effect.set.is_none() && effect.delta <= 0
        })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Damage instructions share the ordinary replacement and simultaneous commit boundary."
)]
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
        let source_id = execution.object_id(source.object)?.to_owned();
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
                    json!({"source":source_id,"source_generation":source.object.generation.0,"target":target,"amount":amount,"battle":false,"attack":false}),
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
            DamageRun {
                instruction: Box::new(instruction.clone()),
                source,
                controller,
                requested,
                remaining,
                hits: vec![],
                facts: vec![],
                choices: vec![],
            },
            None,
        )
    }

    pub(super) fn continue_damage(
        &mut self,
        execution: &mut Execution,
        mut run: DamageRun,
        order: Option<Vec<DamageReplacement>>,
    ) -> Result<()> {
        let mut selected = order;
        while let Some((recipient, mut hit)) = run.remaining.pop() {
            let replacements = if let Some(chosen) = selected.take() {
                chosen
            } else {
                self.damage_replacements(&hit, &BTreeMap::new())?
            };
            if replacements
                .iter()
                .any(|replacement| replacement.consumption.is_some())
            {
                return Err(unsupported("typed limited damage replacement"));
            }
            if !order_independent(&replacements) && run.choices.is_empty() {
                let actor = match recipient {
                    RuntimeDamageRecipient::Leader { player } => player,
                    RuntimeDamageRecipient::Object { object } => {
                        player_id(&self.object(execution.object_id(object)?)?.controller)?
                    }
                };
                let options = replacements
                    .iter()
                    .zip(0_u64..)
                    .map(|(replacement, handle)| {
                        let provider = string(&replacement.reference["source"]);
                        let (card, name) = if let (Some(card), Some(name)) = (
                            replacement.reference["card"].as_str(),
                            replacement.reference["name"].as_str(),
                        ) {
                            (card.to_owned(), name.to_owned())
                        } else {
                            // Installed candidates are pure prevention and never need a public choice.
                            (
                                self.object(provider)?.card.clone(),
                                self.card_name(provider)?,
                            )
                        };
                        Ok(ReplacementOption {
                            handle,
                            card,
                            name,
                            line: replacement.reference["line"].as_u64(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                run.remaining.push((recipient, hit));
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
            let ordered = replacements
                .iter()
                .find(|effect| effect.prevent)
                .cloned()
                .map_or(replacements, |prevention| vec![prevention]);
            let (actual, _) = DamageReplacement::fold_order(int(&hit["amount"]), ordered)?;
            if actual == 0 {
                continue;
            }
            self.internal_damage_arithmetic(string(&hit["target"]), actual)?;
            hit["amount"] = json!(actual);
            run.hits.push(hit);
            run.facts.push(DamageEvent {
                source: run.source,
                recipient,
                amount: PositiveInt(
                    NonZeroU64::new(u64::try_from(actual).map_err(invalid)?)
                        .ok_or_else(|| invalid("positive actual damage"))?,
                ),
                flags: DamageFlags::Ability,
                controller: run.controller,
            });
        }
        let actual = run.facts.iter().try_fold(0_u64, |sum, fact| {
            sum.checked_add(fact.amount.0.get())
                .ok_or_else(|| invalid("damage outcome overflow"))
        })?;
        self.apply_damage(&run.hits, &execution.frame(), Some(&run.facts))?;
        execution
            .facts
            .extend(run.facts.into_iter().map(Fact::Damage));
        execution.receipts.insert(
            run.instruction.receipt,
            InstructionOutcome::Damage(DamageQuantityOutcome {
                requested: Int(run.requested),
                actual: Nat(actual),
            }),
        );
        Ok(())
    }
}
