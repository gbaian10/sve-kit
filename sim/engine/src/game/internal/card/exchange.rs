use super::play::PlayState;
use super::{Execution, Game, Result, int, invalid, seat_name, unsupported};
use crate::game::internal::{
    DistributionMember, InputDomain, InstructionOutcome, Slot, Wait, search,
};
use crate::ir::{
    AbilityKind, Action, ChoiceDomain, CostSpec, FaceId, Int, MoveMethod, MoveOutcome, ObjectRef,
    PlayCostExchange, Posture, PostureEvent, Quantity, QuantityUnit, Selector, ZoneKind,
};
use crate::ir::{
    BanishOutcome, Bool, EventPattern, Instruction, PaymentResult, ReceiptId, RuntimeReceiptId,
    TriggerCondition,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct ExchangeState {
    definition: PlayCostExchange,
    face: FaceId,
    accepted: Option<bool>,
    selected: Option<Vec<ObjectRef>>,
    pay_pp: Option<i64>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MaterialWait {
    members: Vec<ObjectRef>,
    count: usize,
}
impl MaterialWait {
    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if handles.len() != self.count {
            return Err(invalid("material cardinality"));
        }
        let mut unique = BTreeSet::new();
        handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate material handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign material handle"))
            })
            .collect()
    }
}

pub(in crate::game::internal) fn accept(execution: &mut Execution, accepted: bool) -> Result<()> {
    execution
        .play
        .as_mut()
        .and_then(|play| play.exchange.as_mut())
        .ok_or_else(|| invalid("missing exchange decision"))?
        .accepted = Some(accepted);
    Ok(())
}
pub(in crate::game::internal) fn select(
    execution: &mut Execution,
    members: Vec<ObjectRef>,
) -> Result<()> {
    let exchange = execution
        .play
        .as_mut()
        .and_then(|play| play.exchange.as_mut())
        .ok_or_else(|| invalid("missing exchange materials"))?;
    let ChoiceDomain::Objects {
        binding,
        constraint,
        ..
    } = &exchange.definition.materials.domain
    else {
        return Err(invalid("missing material binding"));
    };
    let on_field_at_capture = matches!(
        constraint.groups.first().map(|group| &group.selector),
        Some(Selector::Zone {
            zone: ZoneKind::Field,
            ..
        })
    );
    execution.slots.insert(
        binding.key(),
        Slot::Objects {
            members: members.clone(),
            on_field_at_capture,
        },
    );
    exchange.selected = Some(members);
    Ok(())
}

fn step(exchange: &PlayCostExchange) -> Result<&Instruction> {
    let CostSpec::Ordered { steps } = &exchange.cost else {
        return Err(unsupported("exchange costs"));
    };
    let [step] = steps.as_slice() else {
        return Err(unsupported("one exchange action"));
    };
    Ok(&step.instruction)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Exchange shares the card play transaction and public state."
)]
impl Game {
    pub(super) fn internal_exchange(&self, execution: &Execution) -> Result<Option<ExchangeState>> {
        let object = self.object(&execution.context.source)?;
        let card = self
            .card_runtime
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("unloaded card"))?;
        let Some((face, exchange)) = card.card().card_play_costs.iter().find_map(|face| {
            face.node
                .exchange
                .as_ref()
                .map(|exchange| (&face.node.face, exchange))
        }) else {
            return Ok(None);
        };
        if !self.card_runtime.is_empty() {
            for holder in self.state.objects.keys() {
                self.require_internal_abilities(holder, "exchange payment subscriptions")?;
            }
        }
        if self.card_runtime.quick_start {
            return Err(unsupported("exchange Quick payment"));
        }
        // A definition that can consume payment facts is unsafe even when its holder is hidden.
        for definition in self.card_runtime.cards.values() {
            for ability in &definition.card().abilities {
                if ability.node.trigger.as_ref().is_some_and(|trigger| {
                    !matches!(
                        trigger.on,
                        TriggerCondition::Event {
                            pattern: EventPattern::EnterField { .. } | EventPattern::Phase { .. }
                        }
                    )
                }) {
                    return Err(unsupported("exchange payment or material triggers"));
                }
            }
        }
        let state = ExchangeState {
            face: face.clone(),
            definition: exchange.node.clone(),
            accepted: None,
            selected: None,
            pay_pp: None,
        };
        self.exchange_candidates(execution, &state.definition)?;
        Ok(Some(state))
    }

    pub(super) fn internal_exchange_decision(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<bool> {
        let Some(exchange) = &play.exchange else {
            return Ok(false);
        };
        if exchange.accepted.is_some() {
            return Ok(false);
        }
        self.internal_wait(
            execution,
            exchange.definition.decision,
            execution.context.controller,
            InputDomain::OptionalCost,
            Wait::OptionalCost,
        )?;
        Ok(true)
    }

    fn exchange_candidates(
        &self,
        execution: &Execution,
        exchange: &PlayCostExchange,
    ) -> Result<Vec<ObjectRef>> {
        let ChoiceDomain::Objects { constraint, .. } = &exchange.materials.domain else {
            return Err(unsupported("exchange materials"));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("exchange group"));
        };
        let Selector::Zone {
            zone,
            binding,
            predicate,
            players,
            ..
        } = &group.selector
        else {
            return Err(unsupported("exchange selector"));
        };
        if execution.players(self, players)? != [execution.context.controller] {
            return Err(unsupported("exchange material player bounds"));
        }
        search::check_predicate(predicate, *binding)?;
        let posture = matches!(step(exchange)?.action, Action::Posture { .. });
        let zone = match zone {
            ZoneKind::Field => "field",
            ZoneKind::Cemetery => "cemetery",
            _ => return Err(unsupported("private exchange materials")),
        };
        let mut members = Vec::new();
        for id in self.zone_ids(seat_name(execution.context.controller)?, zone) {
            if !search::matches_candidate(self, &id, predicate)? {
                continue;
            }
            let object = self.object(&id)?;
            if posture && object.state["acted"] == true {
                continue;
            }
            if object.controller != seat_name(execution.context.controller)?
                || self.object_type(&id)?.contains("トークン")
                || self.object_type(&id)?.contains("アドバンス")
                || object
                    .state
                    .get("evolved_with")
                    .is_some_and(|value| !value.is_null())
                || object
                    .state
                    .get("links")
                    .is_some_and(|value| !value.is_null())
                || self.state.objects.values().any(|other| {
                    matches!(other.zone.as_str(), "field" | "cemetery")
                        && other.state["equipped_to"] == id
                })
                || self
                    .internal_printed_abilities(&id)?
                    .iter()
                    .any(|ability| ability.node.kind == AbilityKind::Triggered)
                || self.restricted(
                    &id,
                    if posture {
                        "act"
                    } else if zone == "field" {
                        "move"
                    } else {
                        "banish"
                    },
                )?
            {
                return Err(unsupported(
                    "exchange material has unsupported movement, permission or triggers",
                ));
            }
            members.push(execution.reference(self, &id)?);
        }
        Ok(members)
    }

    pub(super) fn internal_exchange_affordable(
        &self,
        execution: &Execution,
        play: &PlayState,
    ) -> Result<bool> {
        let exchange = play
            .exchange
            .as_ref()
            .ok_or_else(|| invalid("missing exchange preflight"))?;
        let accepted = exchange.accepted == Some(true);
        let pay_pp = self.internal_card_play_cost_with(
            execution,
            accepted.then_some(&exchange.definition.change),
        )?;
        if int(&self.player(seat_name(execution.context.controller)?)?.pp["current"]) < pay_pp {
            return Ok(false);
        }
        if accepted {
            let ChoiceDomain::Objects { constraint, .. } = &exchange.definition.materials.domain
            else {
                return Err(unsupported("exchange material preflight"));
            };
            let members = self.exchange_candidates(execution, &exchange.definition)?;
            return Ok(
                u64::try_from(members.len()).map_err(invalid)? >= constraint.cardinality.minimum.0
            );
        }
        Ok(true)
    }

    pub(super) fn internal_exchange_materials(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
    ) -> Result<bool> {
        let Some(exchange) = &mut play.exchange else {
            return Ok(false);
        };
        if exchange.pay_pp.is_none() {
            exchange.pay_pp = Some(self.internal_card_play_cost_with(
                execution,
                (exchange.accepted == Some(true)).then_some(&exchange.definition.change),
            )?);
        }
        if exchange.accepted != Some(true) || exchange.selected.is_some() {
            return Ok(false);
        }
        let members = self.exchange_candidates(execution, &exchange.definition)?;
        let ChoiceDomain::Objects { constraint, .. } = &exchange.definition.materials.domain else {
            return Err(unsupported("exchange domain"));
        };
        let count = usize::try_from(constraint.cardinality.minimum.0).map_err(invalid)?;
        if members.len() < count {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient exchange materials".into(),
            ));
        }
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                Ok(DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: execution.context.controller,
                    card: Some(self.object(id)?.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.internal_wait(
            execution,
            exchange.definition.materials.id,
            execution.context.controller,
            InputDomain::CostMaterials(super::TargetDomain {
                members: public,
                cardinality: constraint.cardinality,
            }),
            Wait::CostMaterials(MaterialWait { members, count }),
        )?;
        Ok(true)
    }

    pub(super) fn internal_exchange_payment(
        &mut self,
        execution: &mut Execution,
        play: &PlayState,
    ) -> Result<()> {
        let exchange = play.exchange.as_ref();
        let pay_pp = exchange
            .and_then(|state| state.pay_pp)
            .map_or_else(|| self.internal_card_play_cost(execution), Ok)?;
        if let Some(exchange) = exchange.filter(|state| state.accepted == Some(true)) {
            let selected = exchange
                .selected
                .as_ref()
                .ok_or_else(|| invalid("missing exchange selection"))?;
            let candidates = self.exchange_candidates(execution, &exchange.definition)?;
            if selected.iter().any(|member| !candidates.contains(member)) {
                return Err(crate::EngineFailure::IllegalPlay(
                    "exchange materials changed".into(),
                ));
            }
        }
        let quantity = self.internal_pay_pp(execution.context.controller, Int(pay_pp))?;
        let intrinsic = if execution.field_entry {
            ReceiptId(0)
        } else {
            let CostSpec::Ordered { steps } = &play.costs else {
                return Err(unsupported("intrinsic costs"));
            };
            steps
                .first()
                .ok_or_else(|| invalid("missing intrinsic payment"))?
                .instruction
                .receipt
        };
        execution.receipts.insert(
            intrinsic,
            InstructionOutcome::Payment {
                quantity,
                subjects: vec![],
            },
        );
        if let Some(exchange) = exchange {
            let complete = exchange.accepted == Some(true);
            if complete {
                self.pay_exchange_materials(execution, exchange)?;
            }
            execution.receipts.insert(
                exchange.definition.receipt,
                InstructionOutcome::ExchangePayment(PaymentResult {
                    complete: Bool(complete),
                    steps: if complete {
                        vec![
                            RuntimeReceiptId {
                                execution: execution.id,
                                slot: intrinsic,
                            },
                            RuntimeReceiptId {
                                execution: execution.id,
                                slot: step(&exchange.definition)?.receipt,
                            },
                        ]
                    } else {
                        vec![]
                    },
                }),
            );
            execution.card_payment = Some((exchange.face.clone(), exchange.definition.receipt));
        }
        Ok(())
    }
    fn pay_exchange_materials(
        &mut self,
        execution: &mut Execution,
        exchange: &ExchangeState,
    ) -> Result<()> {
        let members = exchange
            .selected
            .as_ref()
            .ok_or_else(|| invalid("missing materials"))?;
        let instruction = step(&exchange.definition)?;
        let outcome = match &instruction.action {
            Action::Posture { .. } => {
                let ids = members
                    .iter()
                    .map(|member| execution.object_id(*member).map(str::to_owned))
                    .collect::<Result<Vec<_>>>()?;
                if self.change_posture(&ids, true, &execution.frame().cause)? != ids {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "incomplete posture exchange".into(),
                    ));
                }
                for object in members {
                    execution.facts.push(super::Fact::Posture(PostureEvent {
                        object: *object,
                        before: Posture::Standing,
                        after: Posture::Acted,
                    }));
                }
                InstructionOutcome::Payment {
                    quantity: Quantity {
                        amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                        unit: QuantityUnit::Cards,
                    },
                    subjects: members.clone(),
                }
            }
            Action::Move { .. } | Action::Banish { .. } => {
                let banish = matches!(instruction.action, Action::Banish { .. });
                let (moved, successful) = self.internal_move_members(
                    execution,
                    members,
                    if banish { "banish" } else { "cemetery" },
                    if banish {
                        ZoneKind::Banish
                    } else {
                        ZoneKind::Cemetery
                    },
                    if banish {
                        MoveMethod::Banish
                    } else {
                        MoveMethod::Put
                    },
                )?;
                if moved.len() != members.len() || moved.iter().any(|result| result.after.is_none())
                {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "incomplete movement exchange".into(),
                    ));
                }
                if banish {
                    InstructionOutcome::Banish(BanishOutcome {
                        successful,
                        moves: moved,
                    })
                } else {
                    InstructionOutcome::Move(MoveOutcome { moved })
                }
            }
            _ => return Err(unsupported("exchange action")),
        };
        execution.receipts.insert(instruction.receipt, outcome);
        Ok(())
    }
}
