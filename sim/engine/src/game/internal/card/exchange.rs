use super::play::PlayState;
use super::{Execution, Game, Result, int, invalid, seat_name, unsupported};
use crate::game::internal::{
    DistributionMember, InputDomain, InstructionOutcome, Slot, Wait, search,
};
use crate::ir::exchange as exchange_shape;
use crate::ir::{
    AbilityKind, Action, Cardinality, ChoiceDomain, CostSpec, Decision, DiscardOutcome, FaceId,
    Int, MoveMethod, MoveOutcome, Nat, ObjectRef, PlayCostExchange, Posture, PostureEvent,
    Quantity, QuantityUnit, Selector, ZoneKind,
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
    route: Option<usize>,
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
    } = &material(exchange)?.domain
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

fn route_step(exchange: &PlayCostExchange, index: usize) -> Result<&Instruction> {
    exchange
        .routes()
        .and_then(|routes| routes.get(index).map(|(_, step)| step.instruction.as_ref()))
        .ok_or_else(|| unsupported("exchange route cost"))
}

fn material(exchange: &ExchangeState) -> Result<&Decision> {
    let index = exchange
        .route
        .ok_or_else(|| invalid("missing exchange route"))?;
    exchange
        .definition
        .materials
        .get(index)
        .ok_or_else(|| invalid("foreign exchange route"))
}

fn step(exchange: &ExchangeState) -> Result<&Instruction> {
    route_step(&exchange.definition, exchange.route.unwrap_or(0))
}

pub(in crate::game::internal) fn choose_route(
    execution: &mut Execution,
    index: usize,
) -> Result<()> {
    let exchange = execution
        .play
        .as_mut()
        .and_then(|play| play.exchange.as_mut())
        .ok_or_else(|| invalid("missing exchange route"))?;
    if exchange.accepted != Some(true)
        || exchange.route.is_some()
        || index >= exchange.definition.materials.len()
    {
        return Err(invalid("foreign exchange route"));
    }
    exchange.route = Some(index);
    Ok(())
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
            .definitions
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
        let routes = exchange
            .node
            .routes()
            .ok_or_else(|| unsupported("exchange routes"))?;
        let hand_payment = routes
            .iter()
            .all(|(_, step)| matches!(step.instruction.action, Action::Discard { .. }));
        let posture_payment = routes
            .iter()
            .all(|(_, step)| matches!(step.instruction.action, Action::Posture { .. }));
        // A definition that can consume payment facts is unsafe even when its holder is hidden.
        for definition in self.card_runtime.definitions.cards.values() {
            for ability in &definition.card().abilities {
                if ability.node.trigger.as_ref().is_some_and(|trigger| {
                    !(matches!(
                        trigger.on,
                        TriggerCondition::Event {
                            pattern: EventPattern::EnterField { .. } | EventPattern::Phase { .. }
                        }
                    ) || ((hand_payment || posture_payment)
                        && matches!(
                            trigger.on,
                            TriggerCondition::Event {
                                pattern: EventPattern::LeaveField { .. }
                            }
                        )))
                }) {
                    return Err(unsupported("exchange payment or material triggers"));
                }
            }
        }
        let state = ExchangeState {
            face: face.clone(),
            definition: exchange.node.clone(),
            accepted: None,
            route: (exchange.node.materials.len() == 1).then_some(0),
            selected: None,
            pay_pp: None,
        };
        for index in 0..state.definition.materials.len() {
            self.exchange_candidates(execution, &state.definition, index)?;
        }
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
        index: usize,
    ) -> Result<Vec<ObjectRef>> {
        let decision = exchange
            .materials
            .get(index)
            .ok_or_else(|| unsupported("exchange material route"))?;
        let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
            return Err(unsupported("exchange materials"));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("exchange group"));
        };
        let Selector::Zone {
            zone,
            binding,
            players,
            ..
        } = &group.selector
        else {
            return Err(unsupported("exchange selector"));
        };
        if execution.players(self, players)? != [execution.context.controller] {
            return Err(unsupported("exchange material player bounds"));
        }
        let (predicate, standing) = exchange_shape::predicate(&group.selector)
            .ok_or_else(|| unsupported("exchange predicate"))?;
        search::check_predicate(predicate, *binding)?;
        let action = &route_step(exchange, index)?.action;
        let posture = matches!(action, Action::Posture { .. });
        let token_put = exchange.materials.len() == 2
            && execution.field_entry
            && matches!(action, Action::Move { .. })
            && *zone == ZoneKind::Field;
        let zone = match zone {
            ZoneKind::Field => "field",
            ZoneKind::Cemetery => "cemetery",
            ZoneKind::Hand => "hand",
            ZoneKind::Ex => "ex",
            _ => return Err(unsupported("exchange material zone")),
        };
        let controller = seat_name(execution.context.controller)?;
        let mut members = Vec::new();
        for id in self.zone_ids(controller, zone) {
            let object = self.object(&id)?;
            if (zone == "hand" && id == execution.context.source)
                || !search::matches_candidate(self, &id, predicate)?
                || ((posture || standing) && object.state["acted"] == true)
            {
                continue;
            }
            if zone == "hand" {
                if object.owner != controller || object.controller != controller {
                    return Err(invalid("foreign hand exchange material"));
                }
                members.push(execution.reference(self, &id)?);
                continue;
            }
            if object.controller != controller
                || (!posture && !token_put && self.object_type(&id)?.contains("トークン"))
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
                || self.internal_printed_abilities(&id)?.iter().any(|ability| {
                    ability.node.kind == AbilityKind::Triggered
                        && !(posture
                            && ability.node.trigger.as_ref().is_some_and(|trigger| {
                                matches!(
                                    trigger.on,
                                    TriggerCondition::Event {
                                        pattern: EventPattern::LeaveField { .. }
                                    }
                                )
                            }))
                })
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

    pub(in crate::game::internal) fn internal_exchange_token_cost_authorized(
        &self,
        execution: &Execution,
        id: &str,
        destination: &str,
    ) -> Result<bool> {
        if !execution.field_entry
            || destination != "cemetery"
            || !matches!(
                self.object_type(id)?,
                "フォロワー・トークン" | "アミュレット・トークン"
            )
        {
            return Ok(false);
        }
        let object = self.object(id)?;
        let seat = seat_name(execution.context.controller)?;
        if object.zone != "field" || object.owner != seat || object.controller != seat {
            return Ok(false);
        }
        let source = self.object(&execution.context.source)?;
        if source.zone != "resolution" || source.generation != execution.context.generation.0 {
            return Ok(false);
        }
        let Some(card) = self.card_runtime.definitions.cards.get(&source.card) else {
            return Ok(false);
        };
        let reference = execution.reference(self, id)?;
        for face in &card.card().card_play_costs {
            let Some(exchange) = &face.node.exchange else {
                continue;
            };
            if !exchange_shape::pair(&exchange.node) {
                continue;
            }
            let routes = exchange
                .node
                .routes()
                .ok_or_else(|| unsupported("exchange route authority"))?;
            for (decision, step) in routes {
                if !matches!(step.instruction.action, Action::Move { .. }) {
                    continue;
                }
                let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
                    continue;
                };
                if matches!(execution.slots.get(&binding.key()),Some(Slot::Objects {members,on_field_at_capture:true}) if members.contains(&reference))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
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
            return Ok(!self
                .exchange_affordable_routes(execution, exchange)?
                .is_empty());
        }
        Ok(true)
    }

    fn exchange_affordable_routes(
        &self,
        execution: &Execution,
        exchange: &ExchangeState,
    ) -> Result<Vec<usize>> {
        let mut affordable = Vec::new();
        for (index, decision) in exchange.definition.materials.iter().enumerate() {
            let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
                return Err(unsupported("exchange route domain"));
            };
            if u64::try_from(
                self.exchange_candidates(execution, &exchange.definition, index)?
                    .len(),
            )
            .map_err(invalid)?
                >= constraint.cardinality.minimum.0
            {
                affordable.push(index);
            }
        }
        Ok(affordable)
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
        if exchange.route.is_none() {
            let affordable = self.exchange_affordable_routes(execution, exchange)?;
            let minimum = *affordable.first().ok_or_else(|| {
                crate::EngineFailure::IllegalPlay("insufficient exchange materials".into())
            })?;
            let maximum = *affordable
                .last()
                .ok_or_else(|| invalid("empty exchange routes"))?;
            self.internal_wait(
                execution,
                exchange
                    .definition
                    .route
                    .ok_or_else(|| invalid("missing route decision"))?,
                execution.context.controller,
                InputDomain::Integer(Cardinality {
                    minimum: Nat(u64::try_from(minimum).map_err(invalid)?),
                    maximum: Some(Nat(u64::try_from(maximum).map_err(invalid)?)),
                }),
                Wait::ExchangeRoute { affordable },
            )?;
            return Ok(true);
        }
        let members = self.exchange_candidates(
            execution,
            &exchange.definition,
            exchange
                .route
                .ok_or_else(|| invalid("missing exchange route"))?,
        )?;
        let decision = material(exchange)?;
        let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
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
            decision.id,
            execution.context.controller,
            InputDomain::CostMaterials(super::TargetDomain {
                distinct_names: false,
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
            let candidates = self.exchange_candidates(
                execution,
                &exchange.definition,
                exchange
                    .route
                    .ok_or_else(|| invalid("missing selected route"))?,
            )?;
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
                characters: vec![],
                quantity,
                subjects: vec![],
            },
        );
        if let Some(exchange) = exchange {
            let complete = exchange.accepted == Some(true);
            if complete {
                self.pay_exchange_materials(execution, exchange)?;
            }
            if !complete && matches!(step(exchange)?.action, Action::Discard { .. }) {
                execution.receipts.insert(
                    step(exchange)?.receipt,
                    InstructionOutcome::Discard(DiscardOutcome {
                        successful: vec![],
                        moves: vec![],
                    }),
                );
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
                                slot: step(exchange)?.receipt,
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
        let instruction = step(exchange)?;
        let outcome = match &instruction.action {
            Action::Discard { .. } => {
                let outcome = self.internal_discard_members(execution, members)?;
                if outcome.moves.len() != members.len()
                    || outcome
                        .moves
                        .iter()
                        .any(|movement| movement.after.is_none())
                {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "incomplete discard exchange".into(),
                    ));
                }
                InstructionOutcome::Discard(outcome)
            }
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
                    characters: vec![],
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
                    false,
                    None,
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
