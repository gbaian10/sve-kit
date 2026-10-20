//! Authority-only D1 entry. No author input, public capability or wire program is admitted.

#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Closed requirement admission rejects all other runtime shapes before mutation."
)]

use super::{Frame, Game, View, int};
use crate::ir::{
    Action, Arithmetic, Binding, Bool, Capture, CaptureRef, CardQuantityOutcome, Cardinality,
    ChoiceDomain, Comparison, Control, DamageAmount, DamageEvent, DamageFlags,
    DamageQuantityOutcome, DamageRecipient, DecisionId, DecisionKind, Declinable, DestroyOutcome,
    DrawEvent, ExecutionId, Expr, Generation, InstallationSubjects, Instruction,
    InstructionContext, Int, IntFrozen, IntOperation, IntRead, Located, LookOutcome,
    MeasuredReceipt, MoveOutcome, Nat, NatFrozen, NatOperation, NatRead, Node, ObjectFrozen,
    ObjectId, ObjectOperand, ObjectRef, ObjectView, OrderOutcome, PlayerId, PlayerRef,
    PlayerRelation, PlayerResource, PlayerSet, PositiveInt, Predicate, Program, ReceiptId,
    ReceiptLocation, RecipientSet, Requirements, ResolvedRecipients, RuntimeDamageRecipient,
    RuntimeObjectView, ScopeId, Selector, SnapshotId, SourceSpan, StepwiseAnswer, StepwiseMode,
    Visibility, WellFormedProgram, ZoneKind,
};
use crate::{EngineFailure, Result, invalid};
use alloc::collections::{BTreeMap, VecDeque};
use core::{num::NonZeroU64, result};
use serde::ser::Error as _;
use serde::{Serialize as _, Serializer};
use serde_json::{Value, json};

mod admission;
mod movement;
mod ordering;
mod values;

pub use movement::MovementFact;
pub use ordering::OrderCard;

/// Internal context pinned to the game's immutable catalogue and one source generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    /// Physical source in the prototype authority, never an author identifier.
    pub source: String,
    /// The generation that owns this execution.
    pub generation: Generation,
    /// Controller fixed for this execution, seat 1 or 2.
    pub controller: PlayerId,
}

/// Unforgeable-through-wire input coordinate. Every new question advances its epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionToken {
    /// Execution identity allocated by the authority.
    pub execution: ExecutionId,
    /// Instruction-local decision point.
    pub decision: DecisionId,
    /// One occurrence, including each before-draw question.
    pub epoch: u64,
}

/// Authorized input without receipts, bindings or the pending program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionView {
    /// Required response coordinate.
    pub token: DecisionToken,
    /// Authorized actor.
    pub actor: PlayerId,
    /// Input shape; no hidden card identities.
    pub domain: InputDomain,
}

/// First-slice input shapes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputDomain {
    /// Continue or stop before exactly one draw attempt.
    BeforeDraw,
    /// One natural number in the checked inclusive range.
    Integer(Cardinality),
    /// Token-scoped handles and printed information, visible only to the chooser.
    Order(Vec<OrderCard>),
}

/// Internal answer, checked against actor and the entire decision token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Answer the current before-draw occurrence.
    Stepwise(StepwiseAnswer),
    /// Answer a bounded integer decision.
    Integer(Nat),
    /// Complete permutation of this question's handles, in top-to-bottom order.
    Order(Vec<u64>),
}

/// Typed instruction outcomes retained only by the authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionOutcome {
    /// One outcome for the whole draw instruction.
    Cards(CardQuantityOutcome),
    /// Sum of positive damage actually applied to still-valid recipients.
    Damage(DamageQuantityOutcome),
    /// Ordinary movement results, without destruction semantics.
    Move(MoveOutcome),
    /// Destruction successes and every attempted movement.
    Destroy(DestroyOutcome),
    /// Fixed generations protected until this resolution ends.
    Installed(Vec<ObjectRef>),
    /// Objects observed without movement or a generation change.
    Look(LookOutcome),
    /// Successful same-deck ordering; never a cross-zone movement.
    Order(OrderOutcome),
}

/// Authority result. This type intentionally has no player/wire serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Whether resolution reached its rules checkpoint.
    pub complete: bool,
    /// Completed instruction results indexed by their typed producer's slot.
    pub receipts: BTreeMap<ReceiptId, InstructionOutcome>,
    /// Typed facts; private draw objects must be projected before delivery.
    pub facts: Vec<Fact>,
}

/// First-slice physical facts, sharing the prototype mutation and causal pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    /// A successful individual draw.
    Draw(DrawEvent),
    /// A positive damage application.
    Damage(DamageEvent),
    /// One physical movement, including its action method and causal execution.
    Movement(Box<MovementFact>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Slot {
    Integer(Int),
    Natural(Nat),
    Boolean(Bool),
    Objects(Vec<ObjectRef>),
}

#[derive(Clone, Debug)]
enum Wait {
    Order(ordering::OrderWait),
    Integer {
        binding: Binding<Nat>,
        bounds: Cardinality,
    },
    Draw {
        instruction: Box<Instruction>,
        player: PlayerId,
        requested: Int,
        actual: Nat,
        remaining: u64,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Execution {
    context: Context,
    id: ExecutionId,
    ids: BTreeMap<ObjectId, String>,
    todo: VecDeque<Node>,
    slots: BTreeMap<(ScopeId, u64), Slot>,
    receipts: BTreeMap<ReceiptId, InstructionOutcome>,
    facts: Vec<Fact>,
    waiting: Option<(DecisionView, Wait)>,
    destroy_guards: Vec<ObjectRef>,
    look_viewers: BTreeMap<ReceiptId, PlayerId>,
}

#[expect(
    clippy::ref_option,
    reason = "Serde serialize_with requires a reference to the field."
)]
pub(super) fn serialize_idle<S>(
    value: &Option<Execution>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if value.is_some() {
        return Err(S::Error::custom(
            "active internal IR executions have no save format",
        ));
    }
    Option::<()>::None.serialize(serializer)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Internal execution shares the authoritative game and its rule primitives."
)]
impl Game {
    /// Execute a fully checked closed body after checking every runtime requirement.
    ///
    /// # Errors
    /// Rejects unsupported whole trees, incompatible contexts and evaluation failures atomically.
    pub fn execute_internal(
        &mut self,
        program: &WellFormedProgram,
        source: &SourceSpan,
        context: Context,
    ) -> Result<Progress> {
        admission::check(program, source)?;
        self.internal_context(&context)?;
        let before = self.clone();
        let result = (|| {
            self.state.ir_epoch = self
                .state
                .ir_epoch
                .checked_add(1)
                .ok_or_else(|| invalid("IR epoch exhausted"))?;
            let mut execution = Execution {
                context,
                id: ExecutionId(self.state.ir_epoch),
                ids: self.internal_identities()?,
                todo: program
                    .program()
                    .steps
                    .iter()
                    .map(|step| step.node.clone())
                    .collect(),
                slots: BTreeMap::new(),
                receipts: BTreeMap::new(),
                facts: Vec::new(),
                waiting: None,
                destroy_guards: Vec::new(),
                look_viewers: BTreeMap::new(),
            };
            self.internal_run(&mut execution)?;
            let progress = execution.progress();
            if !progress.complete {
                self.typed = Some(execution);
            }
            Ok(progress)
        })();
        if result.is_err() {
            *self = before;
        }
        result
    }

    /// Resume only the exact outstanding occurrence and its authorized actor.
    ///
    /// # Errors
    /// Invalid answers and subsequent execution failures restore the complete authority.
    pub fn answer_internal(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
    ) -> Result<Progress> {
        let before = self.clone();
        let result = (|| {
            let mut execution = self
                .typed
                .take()
                .ok_or_else(|| invalid("no internal input point"))?;
            let (view, wait) = execution
                .waiting
                .take()
                .ok_or_else(|| invalid("no pending question"))?;
            if view.actor != actor || view.token != token {
                return Err(invalid("wrong actor or stale internal answer"));
            }
            match (wait, answer) {
                (Wait::Order(wait), Answer::Order(handles)) => {
                    self.internal_order_answer(&mut execution, &wait, &handles)?;
                }
                (Wait::Integer { binding, bounds }, Answer::Integer(number))
                    if contains(bounds, number) =>
                {
                    execution.slots.insert(binding.key(), Slot::Natural(number));
                }
                (
                    Wait::Draw {
                        instruction,
                        player,
                        requested,
                        mut actual,
                        remaining,
                    },
                    Answer::Stepwise(answer),
                ) => {
                    if answer == StepwiseAnswer::Continue {
                        actual.0 = actual
                            .0
                            .checked_add(u64::from(self.internal_draw(
                                &mut execution,
                                &instruction,
                                player,
                                Nat(actual.0.saturating_add(1)),
                            )?))
                            .ok_or_else(|| invalid("draw outcome overflow"))?;
                        if remaining > 1 {
                            self.internal_draw_wait(
                                &mut execution,
                                instruction,
                                player,
                                requested,
                                actual,
                                remaining.saturating_sub(1),
                            )?;
                        } else {
                            execution.receipts.insert(
                                instruction.receipt,
                                InstructionOutcome::Cards(CardQuantityOutcome {
                                    requested,
                                    actual,
                                }),
                            );
                        }
                    } else {
                        execution.receipts.insert(
                            instruction.receipt,
                            InstructionOutcome::Cards(CardQuantityOutcome { requested, actual }),
                        );
                    }
                }
                _ => return Err(invalid("answer is outside the internal domain")),
            }
            self.internal_run(&mut execution)?;
            let progress = execution.progress();
            if !progress.complete {
                self.typed = Some(execution);
            }
            Ok(progress)
        })();
        if result.is_err() {
            *self = before;
        }
        result
    }

    /// Inspect only the current actor's question; authority may inspect it as referee.
    #[must_use]
    pub fn internal_decision(&self, view: View) -> Option<DecisionView> {
        let (question, _) = self.typed.as_ref()?.waiting.as_ref()?;
        view.player()
            .is_none_or(|seat| seat == seat_name(question.actor).unwrap_or_default())
            .then(|| question.clone())
    }

    pub(super) fn internal_decision_packet(&self, view: View) -> Value {
        self.internal_decision(view).map_or(Value::Null, |question| {
            let domain = match question.domain {
                InputDomain::BeforeDraw => json!({"kind":"before-draw"}),
                InputDomain::Integer(bounds) => json!({"kind":"integer","minimum":bounds.minimum.0,"maximum":bounds.maximum.map(|value| value.0)}),
                InputDomain::Order(cards) => json!({"kind":"order","cards":cards.iter().map(|card| json!({"handle":card.handle,"card":card.card,"name":card.name})).collect::<Vec<_>>()}),
            };
            json!({"by":seat_name(question.actor).unwrap_or_default(),"execution":question.token.execution.0,"decision":question.token.decision.0,"epoch":question.token.epoch,"domain":domain})
        })
    }

    fn internal_identities(&mut self) -> Result<BTreeMap<ObjectId, String>> {
        let unassigned = self
            .state
            .objects
            .keys()
            .filter(|id| !self.state.ir_objects.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in unassigned {
            let number = self.state.next_ir_object;
            self.state.next_ir_object = number
                .checked_add(1)
                .ok_or_else(|| invalid("IR object identities exhausted"))?;
            self.state.ir_objects.insert(id, number);
        }
        let mut ids = BTreeMap::new();
        for (id, &number) in &self.state.ir_objects {
            if number >= self.state.next_ir_object
                || ids.insert(ObjectId(number), id.clone()).is_some()
            {
                return Err(invalid("typed object identity was reused"));
            }
        }
        Ok(ids)
    }

    fn internal_context(&self, context: &Context) -> Result<()> {
        seat_name(context.controller)?;
        let source = self.object(&context.source)?;
        if source.generation != context.generation.0
            || source.controller != seat_name(context.controller)?
        {
            return Err(invalid(
                "source context does not match its current generation/controller",
            ));
        }
        if self.typed.is_some()
            || self.state.frame.is_some()
            || self.state.prompt.is_some()
            || !self.state.pending.is_empty()
            || !self.state.continuous.is_empty()
            || !self.state.delayed.is_empty()
            || self.state.game["ended"] == true
            || self.state.flow["kind"] == "pregame"
        {
            return Err(unsupported("busy or unsupported execution context"));
        }
        // Prototype abilities cannot silently become typed trigger/replacement producers.
        for object in self.state.objects.values() {
            if !self.abilities(&object.id)?.is_empty() || !self.keywords(&object.id)?.is_empty() {
                return Err(unsupported(
                    "context requires prototype abilities or keywords",
                ));
            }
        }
        Ok(())
    }

    fn internal_run(&mut self, execution: &mut Execution) -> Result<()> {
        while execution.waiting.is_none() {
            let Some(node) = execution.todo.pop_front() else {
                self.checks()?;
                return Ok(());
            };
            match node {
                Node::Control {
                    control: Control::Sequence { body },
                } => execution.prepend(body),
                Node::Control {
                    control:
                        Control::If {
                            condition,
                            then_body,
                            else_body,
                        },
                } => {
                    let selected = if execution.boolean(self, &condition)?.0 {
                        Some(then_body)
                    } else {
                        else_body
                    };
                    if let Some(body) = selected {
                        execution.prepend(body);
                    }
                }
                Node::Capture { capture } => execution.capture(self, &capture)?,
                Node::Decision { decision } => {
                    let ChoiceDomain::Integer {
                        cardinality,
                        binding,
                    } = decision.domain
                    else {
                        return Err(unsupported("decision domain"));
                    };
                    let actor = execution.player(self, &decision.actor)?;
                    self.internal_wait(
                        execution,
                        decision.id,
                        actor,
                        InputDomain::Integer(cardinality),
                        Wait::Integer {
                            binding,
                            bounds: cardinality,
                        },
                    )?;
                }
                Node::Instruction { instruction } => {
                    self.internal_instruction(execution, instruction)?;
                }
                Node::Control { .. } => return Err(unsupported("control")),
            }
        }
        Ok(())
    }

    fn internal_wait(
        &mut self,
        execution: &mut Execution,
        decision: DecisionId,
        actor: PlayerId,
        domain: InputDomain,
        wait: Wait,
    ) -> Result<()> {
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let token = DecisionToken {
            execution: execution.id,
            decision,
            epoch: self.state.ir_epoch,
        };
        execution.waiting = Some((
            DecisionView {
                token,
                actor,
                domain,
            },
            wait,
        ));
        Ok(())
    }

    fn internal_draw_wait(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
        player: PlayerId,
        requested: Int,
        actual: Nat,
        remaining: u64,
    ) -> Result<()> {
        let Action::Draw {
            mode: StepwiseMode::UpToStepwise { decision },
            ..
        } = &instruction.action
        else {
            return Err(unsupported("draw continuation"));
        };
        self.internal_wait(
            execution,
            decision.id,
            player,
            InputDomain::BeforeDraw,
            Wait::Draw {
                instruction,
                player,
                requested,
                actual,
                remaining,
            },
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Draw and uniform damage retain one atomic instruction boundary."
    )]
    fn internal_instruction(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        match &instruction.action {
            Action::Look { .. } => self.internal_look(execution, &instruction)?,
            Action::Order { .. } => self.internal_order(execution, instruction)?,
            Action::Draw {
                player,
                count,
                mode,
            } => {
                let player = execution.player(self, player)?;
                let requested = execution.integer(self, count)?;
                let limit = u64::try_from(requested.0.max(0)).map_err(invalid)?;
                if limit > 10_000 {
                    return Err(unsupported("draw request exceeds internal execution limit"));
                }
                if matches!(mode, StepwiseMode::UpToStepwise { .. }) && limit > 0 {
                    return self.internal_draw_wait(
                        execution,
                        instruction,
                        player,
                        requested,
                        Nat(0),
                        limit,
                    );
                }
                let mut actual = 0_u64;
                for ordinal in 1..=limit {
                    actual = actual
                        .checked_add(u64::from(self.internal_draw(
                            execution,
                            &instruction,
                            player,
                            Nat(ordinal),
                        )?))
                        .ok_or_else(|| invalid("draw outcome overflow"))?;
                }
                execution.receipts.insert(
                    instruction.receipt,
                    InstructionOutcome::Cards(CardQuantityOutcome {
                        requested,
                        actual: Nat(actual),
                    }),
                );
            }
            Action::Damage {
                recipients,
                amount: DamageAmount::Uniform { amount },
                source:
                    ObjectView::Current {
                        object: source_object,
                    },
                ..
            } => {
                let recipients = execution.recipients(self, recipients)?;
                let amount = execution.integer(self, amount)?;
                let source = execution.object(self, source_object)?;
                let source_id = execution.object_id(source)?.to_owned();
                if self.object(&source_id)?.generation != source.generation.0 {
                    return Err(invalid("damage source generation is no longer current"));
                }
                let actor = execution.player(self, &instruction.source.controller)?;
                let snapshot = SnapshotId(self.state.ir_epoch);
                let mut hits = Vec::new();
                let mut facts = Vec::new();
                let mut actual = 0_u64;
                for recipient in recipients.members() {
                    let target = match *recipient {
                        RuntimeDamageRecipient::Leader { player } => {
                            format!("{}.leader", seat_name(player)?)
                        }
                        RuntimeDamageRecipient::Object { object } => {
                            let id = execution.object_id(object)?;
                            let current = self.object(id)?;
                            if current.generation != object.generation.0 || current.zone != "field"
                            {
                                continue;
                            }
                            if !self.object_type(id)?.contains("フォロワー") {
                                continue;
                            }
                            id.to_owned()
                        }
                    };
                    if let Ok(positive) =
                        NonZeroU64::try_from(u64::try_from(amount.0).unwrap_or_default())
                    {
                        actual = actual
                            .checked_add(positive.get())
                            .ok_or_else(|| invalid("damage outcome overflow"))?;
                        hits.push(json!({"source":source_id,"target":target,"amount":amount.0,"battle":false,"attack":false}));
                        facts.push(Fact::Damage(DamageEvent {
                            source: RuntimeObjectView {
                                object: source,
                                snapshot,
                            },
                            recipient: *recipient,
                            amount: PositiveInt(positive),
                            flags: DamageFlags::Ability,
                            controller: actor,
                        }));
                    }
                }
                let frame = execution.frame();
                self.apply_damage(&hits, &frame)?;
                execution.facts.extend(facts);
                execution.receipts.insert(
                    instruction.receipt,
                    InstructionOutcome::Damage(DamageQuantityOutcome {
                        requested: amount,
                        actual: Nat(actual),
                    }),
                );
            }
            Action::Move {
                objects,
                movement,
                reason,
            } => {
                self.internal_move(execution, &instruction, objects, Some((movement, *reason)))?;
            }
            Action::Destroy { objects } => {
                self.internal_move(execution, &instruction, objects, None)?;
            }
            Action::InstallEffect { subjects, .. } => {
                let InstallationSubjects::Objects { objects, .. } = subjects else {
                    return Err(unsupported("installation subjects"));
                };
                let objects = execution.objects(self, objects)?;
                execution.destroy_guards.extend(objects.iter().copied());
                execution
                    .receipts
                    .insert(instruction.receipt, InstructionOutcome::Installed(objects));
            }
            _ => return Err(unsupported("instruction")),
        }
        Ok(())
    }

    fn internal_draw(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        player: PlayerId,
        ordinal: Nat,
    ) -> Result<bool> {
        let seat = seat_name(player)?;
        let before = self.zone_ids(seat, "hand");
        self.draw(seat, &execution.frame())?;
        let after = self.zone_ids(seat, "hand");
        let Some(id) = after.iter().find(|id| !before.contains(id)) else {
            return Ok(false);
        };
        // Historical identity may remain known; it must not locate the new private slot.
        for (viewer, knowledge) in &mut self.state.knowledge {
            if viewer != seat {
                knowledge.located.remove(id);
            }
        }
        let object = execution.reference(self, id)?;
        execution.facts.push(Fact::Draw(DrawEvent {
            player,
            object,
            ordinal,
            instruction: instruction.id,
        }));
        Ok(true)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Value evaluation shares the private execution environment."
)]
impl Execution {
    fn progress(&self) -> Progress {
        Progress {
            complete: self.waiting.is_none() && self.todo.is_empty(),
            receipts: self.receipts.clone(),
            facts: self.facts.clone(),
        }
    }
    fn prepend(&mut self, program: Program) {
        for step in program.steps.into_iter().rev() {
            self.todo.push_front(step.node);
        }
    }
    fn frame(&self) -> Frame {
        Frame {
            source: self.context.source.clone(),
            controller: seat_name(self.context.controller)
                .unwrap_or_default()
                .to_owned(),
            cause: json!({"effect":{"execution":self.id.0}}),
            ..Frame::default()
        }
    }
    fn object_id(&self, object: ObjectRef) -> Result<&str> {
        self.ids
            .get(&object.id)
            .map(String::as_str)
            .ok_or_else(|| invalid("unknown internal object identity"))
    }
    fn reference(&self, game: &Game, id: &str) -> Result<ObjectRef> {
        let object = game.object(id)?;
        let (key, _) = self
            .ids
            .iter()
            .find(|(_, value)| value.as_str() == id)
            .ok_or_else(|| invalid("unregistered internal object identity"))?;
        Ok(ObjectRef {
            id: *key,
            generation: Generation(object.generation),
        })
    }
}

fn contains(bounds: Cardinality, value: Nat) -> bool {
    value.0 >= bounds.minimum.0 && bounds.maximum.is_none_or(|maximum| value.0 <= maximum.0)
}
fn seat_name(player: PlayerId) -> Result<&'static str> {
    match player.seat {
        1 => Ok("P1"),
        2 => Ok("P2"),
        _ => Err(invalid("internal player must be seat 1 or 2")),
    }
}
fn player_id(seat: &str) -> Result<PlayerId> {
    match seat {
        "P1" => Ok(PlayerId { seat: 1 }),
        "P2" => Ok(PlayerId { seat: 2 }),
        _ => Err(invalid("invalid authoritative player")),
    }
}
fn unsupported(shape: &str) -> EngineFailure {
    EngineFailure::Unsupported(format!("internal D1 does not support {shape}"))
}
