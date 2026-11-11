//! Cost inspection reads the same amounts as payment, without running payment.

use super::x_pp::Budget;
use super::{Execution, Game, PlayState, Result, Slot, Wait};
use crate::game::player::Viewer;
use crate::game::player::observation::{
    CardCostQuote, CostGroup, CostGroupKind, CostLine, CostObservation, CostSummary, PendingCost,
    PlayCostQuote, PpQuote, QuoteBasis, seat,
};
use crate::ir::{
    AbilityKind, Action, Binding, CostSpec, CostStep, DecisionKind, DiscardMode, Generation, Int,
    Nat, NumericChange, ObjectId, PlaySlot, Resource, Selector, x_pp,
};
use alloc::collections::{BTreeMap, BTreeSet};

fn pp_binding(costs: &CostSpec) -> Option<Binding<Nat>> {
    let CostSpec::Ordered { steps } = costs else {
        return None;
    };
    steps
        .iter()
        .find_map(|step| match &step.instruction.action {
            Action::Resource {
                amount,
                resource: Resource::PpCurrent,
                ..
            } => x_pp::amount(amount),
            _ => None,
        })
}

fn pp_line(
    game: &Game,
    execution: &Execution,
    costs: &CostSpec,
    card: bool,
    basis: QuoteBasis,
) -> Result<Option<PpQuote>> {
    let CostSpec::Ordered { steps } = costs else {
        return Ok(Some(PpQuote::Unavailable));
    };
    if !steps.iter().any(|step| {
        matches!(
            step.instruction.action,
            Action::Resource {
                resource: Resource::PpCurrent,
                ..
            }
        )
    }) {
        return Ok(None);
    }
    let binding = pp_binding(costs);
    let mut budget = Budget::default();
    budget.include(costs, binding, |amount| {
        if card && basis == QuoteBasis::CurrentContext {
            game.internal_quote_card_cost(execution).map(Int)
        } else {
            game.internal_card_pp_amount(execution, card, amount)
        }
    })?;
    Ok(Some(if budget.copies == 0 {
        PpQuote::Known(budget.fixed)
    } else {
        let x = binding.and_then(|key| match execution.slots.get(&key.key()) {
            Some(Slot::Natural(value)) => Some(value.0),
            _ => None,
        });
        PpQuote::Variable {
            fixed: budget.fixed,
            copies: budget.copies,
            x,
        }
    }))
}

fn selected_count(game: &Game, execution: &Execution, objects: &Selector) -> Option<u64> {
    let members = execution.objects(game, objects).ok()?;
    u64::try_from(members.len()).ok()
}

fn component(game: &Game, execution: &Execution, step: &CostStep) -> Option<CostLine> {
    let action = &step.instruction.action;
    let selected = execution.cost_materials.get(&step.instruction.receipt);
    let selected_total = selected.and_then(|members| u64::try_from(members.len()).ok());
    Some(match action {
        Action::Resource {
            resource: Resource::PpCurrent,
            ..
        } => return None,
        Action::Posture { objects, .. } => CostLine::Posture {
            cards: selected_count(game, execution, objects),
        },
        Action::Life {
            change: NumericChange::Add { amount },
            ..
        } => CostLine::Life {
            change: execution.integer(game, amount).ok().map(|value| value.0),
        },
        Action::Counter { kind, amount, .. } => {
            let name = game.catalog.keyword_name(&kind.0);
            CostLine::Counter {
                name: if name.is_empty() {
                    kind.0.clone()
                } else {
                    name.into()
                },
                count: execution.natural(game, amount).ok().map(|value| value.0),
            }
        }
        Action::Move { objects, .. } => CostLine::Move {
            cards: selected_count(game, execution, objects),
        },
        Action::Banish { objects } => CostLine::Banish {
            cards: selected_count(game, execution, objects),
        },
        Action::Reveal { objects } => CostLine::Reveal {
            cards: selected_count(game, execution, objects),
        },
        Action::Discard { mode, .. } => {
            let cards = selected_total.or_else(|| match mode {
                DiscardMode::Chosen { constraint } => (constraint.cardinality.maximum
                    == Some(constraint.cardinality.minimum))
                .then_some(constraint.cardinality.minimum.0),
                DiscardMode::Random { count, .. } => {
                    let value = execution.integer(game, count).ok()?;
                    u64::try_from(value.0).ok()
                }
                DiscardMode::All { eligible } => selected_count(game, execution, eligible),
                DiscardMode::DownTo { .. } => None,
            });
            CostLine::Discard {
                cards,
                selected: selected.is_some() || matches!(mode, DiscardMode::All { .. }),
            }
        }
        Action::Fusion { materials, .. } => CostLine::Fusion {
            cards: selected_total.or_else(|| {
                (materials.cardinality.maximum == Some(materials.cardinality.minimum))
                    .then_some(materials.cardinality.minimum.0)
            }),
            selected: selected.is_some(),
        },
        _ => CostLine::Other,
    })
}

fn group(
    game: &Game,
    execution: &Execution,
    costs: &CostSpec,
    kind: CostGroupKind,
    card: bool,
    basis: QuoteBasis,
) -> CostGroup {
    let mut lines = vec![];
    match pp_line(game, execution, costs, card, basis) {
        Ok(Some(pp)) => lines.push(CostLine::Pp(pp)),
        Ok(None) => {}
        Err(_) => lines.push(CostLine::Pp(PpQuote::Unavailable)),
    }
    if let CostSpec::Ordered { steps } = costs {
        lines.extend(
            steps
                .iter()
                .filter_map(|step| component(game, execution, step)),
        );
    } else {
        lines.push(CostLine::Other);
    }
    CostGroup { kind, lines }
}

fn summary(
    basis: QuoteBasis,
    groups: Vec<CostGroup>,
    mut pending: BTreeSet<PendingCost>,
) -> CostSummary {
    let mut total = Some(0_i64);
    for group in &groups {
        if let CostGroupKind::Optional { accepted, .. } = group.kind {
            if accepted.is_none() {
                pending.insert(PendingCost::Optional);
            }
            if accepted != Some(true) {
                continue;
            }
        }
        for line in &group.lines {
            match line {
                CostLine::Pp(pp) => {
                    let amount = match *pp {
                        PpQuote::Known(amount) => Some(amount),
                        PpQuote::Variable {
                            fixed,
                            copies,
                            x: Some(x),
                        } => i64::try_from(x)
                            .ok()
                            .and_then(|x| {
                                let copies = i64::try_from(copies).ok()?;
                                x.checked_mul(copies)
                            })
                            .and_then(|value| fixed.checked_add(value)),
                        PpQuote::Variable { x: None, .. } => {
                            pending.insert(PendingCost::X);
                            None
                        }
                        PpQuote::Unavailable => {
                            pending.insert(PendingCost::Unavailable);
                            None
                        }
                    };
                    let combined = total
                        .zip(amount)
                        .map(|(before, amount)| before.checked_add(amount));
                    if combined == Some(None) {
                        pending.insert(PendingCost::Unavailable);
                    }
                    total = combined.flatten();
                    if amount.is_none() && !matches!(pp, PpQuote::Variable { x: None, .. }) {
                        pending.insert(PendingCost::Unavailable);
                    }
                }
                CostLine::Posture { cards: None }
                | CostLine::Move { cards: None }
                | CostLine::Banish { cards: None }
                | CostLine::Reveal { cards: None }
                | CostLine::Discard {
                    selected: false, ..
                }
                | CostLine::Fusion {
                    selected: false, ..
                } => {
                    pending.insert(PendingCost::Materials);
                }
                CostLine::Counter { count: None, .. }
                | CostLine::Life { change: None }
                | CostLine::Other => {
                    pending.insert(PendingCost::Unavailable);
                }
                _ => {}
            }
        }
    }
    CostSummary {
        basis,
        total_pp: pending.is_empty().then_some(total).flatten(),
        groups,
        pending: pending.into_iter().collect(),
    }
}

fn play_cost(play: &PlayState, game: &Game, execution: &Execution) -> CostSummary {
    let basis = QuoteBasis::BeforePayment;
    let mut pending = BTreeSet::new();
    if !play.modes.is_empty() {
        pending.insert(PendingCost::Modes);
    }
    if matches!(
        execution.waiting,
        Some((
            _,
            Wait::MaterialParameter(_) | Wait::CostMaterials(_) | Wait::CostDiscard(_)
        ))
    ) || play.parameters.iter().any(|parameter| {
        matches!(
            parameter.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                ..
            }
        ) && !play.declined_material(parameter)
    }) {
        pending.insert(PendingCost::Materials);
    }
    let mut groups = vec![];
    if let Some(entry) = &play.resource_entry {
        let (pp, cards, use_ep) = entry.observed_cost();
        if use_ep.is_none() {
            pending.insert(PendingCost::EntryResources);
        }
        groups.push(CostGroup {
            kind: CostGroupKind::Original,
            lines: vec![
                CostLine::Pp(PpQuote::Known(pp)),
                CostLine::EntryResources { cards, use_ep },
            ],
        });
    } else if execution.field_entry || play.exchange.is_some() {
        let quoted = play.exchange.as_ref().map_or_else(
            || game.internal_card_play_cost(execution),
            |exchange| {
                pending.extend(exchange.observed_pending());
                exchange.observed_pp(game, execution)
            },
        );
        let pp = quoted.map_or(PpQuote::Unavailable, PpQuote::Known);
        groups.push(CostGroup {
            kind: CostGroupKind::Original,
            lines: vec![CostLine::Pp(pp)],
        });
    } else {
        groups.push(group(
            game,
            execution,
            &play.costs,
            CostGroupKind::Original,
            execution.spell,
            basis,
        ));
    }
    groups.push(group(
        game,
        execution,
        &play.mode_costs,
        CostGroupKind::Mode,
        false,
        basis,
    ));
    if let Some((_, payment)) = play.additional.as_deref() {
        groups.push(group(
            game,
            execution,
            &payment.play.costs,
            CostGroupKind::Additional,
            false,
            basis,
        ));
    }
    for payment in &play.optional {
        groups.push(group(
            game,
            execution,
            &payment.costs,
            CostGroupKind::Optional {
                required: payment.required,
                accepted: payment.accepted,
            },
            false,
            basis,
        ));
    }
    summary(basis, groups, pending)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Cost observations read the same card context and play state as payment."
)]
impl Game {
    pub(in crate::game) fn observed_costs(&self, perspective: Viewer) -> Result<CostObservation> {
        let mut cards = Vec::new();
        let mut candidates = Vec::new();
        for name in ["hand", "ex"] {
            for id in self.zone_ids(seat(perspective), name) {
                let object = self.object(&id)?;
                if self.card_runtime.is_loaded(&object.card)
                    && object.controller == seat(perspective)
                    && self.player_knows_object(perspective, &id)
                {
                    candidates.push(id);
                }
            }
        }
        if !candidates.is_empty() {
            // Quote-local identity allocation never touches the authoritative game or its RNG.
            let mut probe = self.clone();
            let ids = probe.internal_identities().ok();
            for id in candidates {
                let knowledge = self
                    .state
                    .knowledge
                    .get(seat(perspective))
                    .ok_or_else(|| crate::invalid("missing viewer knowledge"))?;
                let card = knowledge
                    .handle(&id)
                    .ok_or_else(|| crate::invalid("missing cost card handle"))?;
                let cost = ids
                    .as_ref()
                    .and_then(|ids| probe.card_cost_preview(&id, perspective, ids.clone()).ok())
                    .unwrap_or_else(|| {
                        summary(
                            QuoteBasis::CurrentContext,
                            vec![CostGroup {
                                kind: CostGroupKind::Original,
                                lines: vec![CostLine::Pp(PpQuote::Unavailable)],
                            }],
                            BTreeSet::new(),
                        )
                    });
                cards.push(CardCostQuote { card, cost });
            }
        }
        let current_play = self
            .typed
            .as_ref()
            .filter(|execution| execution.context.controller == perspective.player())
            .and_then(|execution| {
                execution.play.as_ref().map(|play| PlayCostQuote {
                    source: self
                        .state
                        .knowledge
                        .get(seat(perspective))
                        .and_then(|knowledge| knowledge.handle(&execution.context.source)),
                    cost: play_cost(play, self, execution),
                })
            });
        Ok(CostObservation {
            cards,
            current_play,
        })
    }

    // Admission changes must keep these components aligned with prepare_ability_play.
    fn card_cost_preview(
        &self,
        holder: &str,
        perspective: Viewer,
        ids: BTreeMap<ObjectId, String>,
    ) -> Result<CostSummary> {
        let object = self.object(holder)?;
        let execution = self.card_execution(
            super::Context {
                source: holder.into(),
                generation: Generation(object.generation),
                controller: perspective.player(),
            },
            ids,
        )?;
        let pp = self
            .internal_quote_card_cost(&execution)
            .map_or(PpQuote::Unavailable, PpQuote::Known);
        let mut groups = vec![CostGroup {
            kind: CostGroupKind::Original,
            lines: vec![CostLine::Pp(pp)],
        }];
        let mut pending = BTreeSet::new();
        let card = self
            .card_runtime
            .definitions
            .cards
            .get(&object.card)
            .ok_or_else(|| crate::invalid("unloaded quote card"))?;
        for face in &card.card().card_play_costs {
            if face.node.exchange.is_some() {
                pending.insert(PendingCost::Exchange);
            }
            if let Some(payment) = &face.node.additional {
                groups.push(group(
                    self,
                    &execution,
                    &payment.node.play.costs,
                    CostGroupKind::Additional,
                    false,
                    QuoteBasis::CurrentContext,
                ));
            }
        }
        for ability in card
            .card()
            .abilities
            .iter()
            .filter(|ability| ability.node.kind == AbilityKind::Spell)
        {
            if let Some(mut body) = ability.node.body.clone() {
                if !super::collect_modes(&body, super::conditional_play_modes(&ability.node))?
                    .is_empty()
                {
                    pending.insert(PendingCost::Modes);
                }
                if let Some(optional) = super::super::optional::take(&mut body) {
                    groups.push(group(
                        self,
                        &execution,
                        &optional.costs,
                        CostGroupKind::Optional {
                            required: optional.required,
                            accepted: None,
                        },
                        false,
                        QuoteBasis::CurrentContext,
                    ));
                }
            }
        }
        Ok(summary(QuoteBasis::CurrentContext, groups, pending))
    }
}
