use crate::ir::player_stages;
mod capacity;
pub(super) mod dynamic;
pub(super) mod equipment;

use crate::ir::{
    Capture, CaptureRef, CaptureTime, Cardinality, PlayerRelation, PlayerSet, TextQuantifier,
};
use crate::ir::{KeywordId, ObjectRef, PlayerId, printed_keywords};
use crate::ir::{optional_payment, x_pp};

pub(super) use capacity::{CapacityWait, TransformResume};

pub(super) use dynamic::check_each;

use super::number_plan::CountPlan;
use super::{Execution, Game, InstructionOutcome, Result, Slot, invalid, seat_name, unsupported};
use crate::game::tokens::{prints_for_names, token_combinations};
use crate::ir::events::timing::opponent;
use crate::ir::{
    Ability, Action, Binding, CardKind, CardName, Control, CostSpec, Expr, Instruction, LkiReason,
    Located, NameOperation, Nat, Node, ObjectFrozen, ObjectOperand, ObjectView, PlayerRef, Posture,
    Program, ReceiptId, Requirement, Resource, ResourceOperation, SourceSpan, TokenDestination,
    TokenInitialState, TokenOutcome, TokenSpec, TokenTemplate, ZoneKind, ZoneRef,
};

#[derive(Clone, Debug)]
pub(super) struct Creation {
    receipt: ReceiptId,
    ids: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PaidX(Binding<Nat>);

pub(super) fn paid_x(ability: &Ability) -> Option<PaidX> {
    let play = ability.play.as_ref()?;
    let binding = play.parameters.first().and_then(x_pp::parameter)?;
    let backed = |costs: &CostSpec| {
        matches!(costs, CostSpec::Ordered { steps } if steps.iter().any(|step| {
            matches!(&step.instruction.action, Action::Resource {
                player: PlayerRef::Controller {}, resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay, amount,
            } if x_pp::amount(amount) == Some(binding))
        }))
    };
    (backed(&play.costs)
        || ability
            .body
            .as_ref()
            .and_then(optional_payment::declaration)
            .is_some_and(|(costs, _)| backed(costs)))
    .then_some(PaidX(binding))
}

fn specification(action: &Action) -> Option<(&[TokenSpec], &ZoneRef)> {
    specification_with_x(action, None)
}

fn specification_with_x(action: &Action, x: Option<PaidX>) -> Option<(&[TokenSpec], &ZoneRef)> {
    let Action::Token {
        tokens,
        destination: TokenDestination::Single {
            zone: Expr::Literal(zone),
        },
        initial_state,
        initial_posture: Posture::Standing,
        entry_effects,
        ..
    } = action
    else {
        return None;
    };
    if tokens.is_empty() {
        return None;
    }
    if !(matches!(
        zone.player,
        PlayerRef::Controller {} | PlayerRef::Bound { .. }
    ) || zone.player == opponent())
        || !matches!(zone.kind, ZoneKind::Ex | ZoneKind::Field)
        || !entry_effects.is_empty()
        || match initial_state {
            None => false,
            Some(TokenInitialState::StackCounters {
                amount: Expr::Literal(amount),
            }) => zone.kind != ZoneKind::Field || amount.0 == 0 || i64::try_from(amount.0).is_err(),
            Some(_) => true,
        }
    {
        return None;
    }
    tokens
        .iter()
        .all(|token| match &token.template {
            TokenTemplate::Fixed { .. } => {
                CountPlan::compile_with_x(&token.count, x.map(|paid| paid.0)).is_ok()
                    && (x.is_none()
                        || zone.player == (PlayerRef::Controller {})
                        || CountPlan::compile(&token.count).is_ok())
            }
            TokenTemplate::Name { name, kind } => {
                if !matches!(kind, None | Some(CardKind::Follower)) {
                    return false;
                }
                tokens.len() == 1
                    && token.count == Expr::Literal(Nat(1))
                    && zone.player == (PlayerRef::Controller {})
                    && initial_state.is_none()
                    && (matches!(name, Expr::Captured(_)) || departure_name(name))
            }
        })
        .then_some((tokens, zone))
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TokenPlan {
    instruction: Instruction,
    destination: ZoneRef,
    counts: Vec<CountPlan>,
}
impl TokenPlan {
    pub(in crate::game::internal) fn compile(
        instruction: &Instruction,
        x: Option<PaidX>,
    ) -> Result<Self> {
        let (tokens, destination) = specification_with_x(&instruction.action, x)
            .ok_or_else(|| unsupported("token creation shape"))?;
        Ok(Self {
            instruction: instruction.clone(),
            destination: destination.clone(),
            counts: tokens
                .iter()
                .map(|token| CountPlan::compile_with_x(&token.count, x.map(|paid| paid.0)))
                .collect::<Result<_>>()?,
        })
    }
    pub(in crate::game::internal) const fn instruction(&self) -> &Instruction {
        &self.instruction
    }
}

pub(super) enum TokenAllocation {
    Created(Vec<String>),
    Capacity {
        prints: Vec<String>,
        names: Vec<String>,
        choices: Vec<Vec<String>>,
        available: usize,
    },
}

pub(super) fn departure_name(name: &Expr<CardName>) -> bool {
    let Expr::Compute(operation) = name else {
        return false;
    };
    let NameOperation::UniqueName {
        object:
            ObjectView::LastKnown {
                object,
                reason: LkiReason::ZoneExit,
            },
    } = operation.as_ref()
    else {
        return false;
    };
    matches!(object.as_ref(), ObjectOperand::FromExpression { value }
        if matches!(value.as_ref(), Expr::Frozen(frozen)
            if matches!(frozen.as_ref(), ObjectFrozen::LeaveFieldObjectBefore { .. })))
}

pub(super) fn check_action(action: &Action, source: &SourceSpan, x: Option<PaidX>) -> Result<()> {
    if matches!(action, Action::Token { .. }) && specification_with_x(action, x).is_none() {
        return Err(unsupported(&format!(
            "token creation with admitted counts and destination at {}:{} ({})",
            source.card_no, source.line, source.author_path
        )));
    }
    Ok(())
}

pub(super) fn check_catalog(
    game: &Game,
    action: &Action,
    source: &SourceSpan,
    x: Option<PaidX>,
) -> Result<()> {
    check_action(action, source, x)?;
    if let Some((tokens, zone)) = specification_with_x(action, x) {
        for token in tokens {
            let TokenTemplate::Fixed { template } = &token.template else {
                continue;
            };
            check_template(game, &template.0, zone.kind)?;
            if matches!(
                action,
                Action::Token {
                    initial_state: Some(_),
                    ..
                }
            ) {
                let card = game.catalog.token_print(&template.0)?;
                let definitions = &game.card_runtime.definitions;
                let program = definitions
                    .cards
                    .get(&card)
                    .map(|card| card.checked())
                    .or_else(|| {
                        definitions
                            .current
                            .get(&card)
                            .map(|current| &current.candidate.program)
                    })
                    .ok_or_else(|| unsupported("Stack token definition"))?;
                if !program.card().abilities.iter().any(|ability| {
                    printed_keywords::card_declaration(&ability.node) == Some(KeywordId::Stack)
                }) {
                    return Err(unsupported("Stack initial state requires a Stack token"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check_template(game: &Game, name: &str, zone: ZoneKind) -> Result<()> {
    let card = game.catalog.token_print(name)?;
    if zone == ZoneKind::Field
        && !matches!(
            game.catalog.face(&card, 0)?["card_type"].as_str(),
            Some("フォロワー・トークン" | "アミュレット・トークン")
        )
    {
        return Err(unsupported(
            "field creation requires a follower or amulet token",
        ));
    }
    if !game.card_runtime.has_definition(&card) {
        return Err(unsupported("created token requires an admitted whole card"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Creation uses the shared physical object and movement boundaries."
)]
impl Game {
    pub(super) fn internal_tokens(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let plan = execution.token_plan(instruction)?;
        let destination = &plan.destination;
        let ids = if let Some(ids) = Self::saved_creation(execution, instruction.receipt)? {
            ids
        } else {
            let Some(ids) = self.allocate_tokens(execution, instruction)? else {
                return Ok(());
            };
            ids
        };
        self.commit_tokens(
            execution,
            instruction,
            destination.kind,
            ids,
            Node::Instruction {
                instruction: Box::new(instruction.clone()),
            },
        )
    }

    /// Expands a choice procedure stage by stage, turn player first.
    fn internal_choice_procedure(
        &self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        let (players, binding, stages) = player_stages::choice_procedure(control)
            .ok_or_else(|| invalid("checked choice procedure"))?;
        let mut order = execution.players(self, players)?;
        let active = super::player_id(self.active())?;
        order.sort_by_key(|member| *member != active);
        execution.begin_procedure(super::player_stages::ProcedureLocals::new(
            binding.key(),
            stages,
        ));
        // Each body follows a capture of its player, so a decision waiting inside a body
        // keeps that player bound when the procedure resumes.
        for stage in stages.iter().rev() {
            for member in order.iter().rev() {
                let player = if *member == active {
                    PlayerRef::Active {}
                } else {
                    PlayerRef::RequireOne {
                        players: Box::new(PlayerSet {
                            relation: PlayerRelation::Opponents,
                            relative_to: PlayerRef::Active {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::One,
                        }),
                    }
                };
                execution.prepend(stage.body.clone());
                execution.todo.push_front(Node::Capture {
                    capture: Capture::Player {
                        at: CaptureTime::AtResolution {
                            scope: stage.body.scope,
                        },
                        value: Expr::Literal(player),
                        into: CaptureRef { binding },
                    },
                });
            }
        }
        Ok(())
    }

    pub(super) fn internal_player_stage(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        if player_stages::procedure(control).is_some() {
            return self.internal_nonchoice_player_stages(execution, control);
        }
        if player_stages::choice_procedure(control).is_some() {
            return self.internal_choice_procedure(execution, control);
        }
        if super::movement::player_banish(control).is_some() {
            let previous = self.card_runtime.active_source;
            self.card_runtime.active_source = execution.ability;
            let result = self.internal_player_banish(execution, control);
            self.card_runtime.active_source = previous;
            return result;
        }
        let Control::EachPlayer {
            players,
            player,
            stages,
            ..
        } = control
        else {
            return Err(invalid("token player stage"));
        };
        let [stage] = stages.as_slice() else {
            return Err(invalid("token stage"));
        };
        let [
            Located {
                node: Node::Instruction { instruction },
                ..
            },
        ] = stage.body.steps.as_slice()
        else {
            return Err(invalid("token stage instruction"));
        };
        let plan = execution.token_plan(instruction)?;
        let destination = &plan.destination;
        let previous = self.card_runtime.active_source;
        self.card_runtime.active_source = execution.ability;
        let result = (|| {
            let ids =
                if let Some(ids) = Self::saved_creation(execution, instruction.receipt)? {
                    ids
                } else {
                    let mut members = execution.players(self, players)?;
                    let active = super::player_id(self.active())?;
                    members.sort_by_key(|member| *member != active);
                    let mut ids = Vec::new();
                    for member in members {
                        execution.slots.insert(player.key(), Slot::Player(member));
                        ids.extend(self.allocate_tokens(execution, instruction)?.ok_or_else(
                            || invalid("single-template player stage requested capacity input"),
                        )?);
                    }
                    ids
                };
            self.commit_tokens(
                execution,
                instruction,
                destination.kind,
                ids,
                Node::Control {
                    control: control.clone(),
                },
            )
        })();
        self.card_runtime.active_source = previous;
        result
    }

    fn saved_creation(
        execution: &mut Execution,
        receipt: ReceiptId,
    ) -> Result<Option<Vec<String>>> {
        execution
            .creating
            .take()
            .map(|creation| {
                if creation.receipt != receipt {
                    return Err(invalid("token creation continuation mismatch"));
                }
                Ok(creation.ids)
            })
            .transpose()
    }

    fn allocate_tokens(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<Option<Vec<String>>> {
        let plan = execution.token_plan(instruction)?;
        let actor = execution.player(self, &plan.destination.player)?;
        let Action::Token { tokens, .. } = &plan.instruction.action else {
            return Err(invalid("token plan action"));
        };
        let mut requests = Vec::new();
        for (token, count) in tokens.iter().zip(&plan.counts) {
            let count = count.evaluate(execution, self)?;
            let name = match &token.template {
                TokenTemplate::Fixed { template } => template.0.clone(),
                TokenTemplate::Name { name, .. } => execution.name(self, name)?.0,
            };
            let card = self.catalog.token_print(&name)?;
            if matches!(
                &token.template,
                TokenTemplate::Name {
                    kind: Some(CardKind::Follower),
                    ..
                }
            ) && self.catalog.face(&card, 0)?["card_type"] != "フォロワー・トークン"
            {
                // CR 9.1.2.3 / 1.3.2 constrains a named template by its printed kind.
                continue;
            }
            requests.push((name, count));
        }
        let destination = plan.destination.kind;
        match self.allocate_token_requests(&requests, destination, actor, None)? {
            TokenAllocation::Created(ids) => Ok(Some(ids)),
            TokenAllocation::Capacity {
                prints,
                names,
                choices,
                available,
            } => {
                self.internal_token_capacity_wait(
                    execution,
                    CapacityWait {
                        instruction: instruction.clone(),
                        prints,
                        names,
                        choices,
                        zone: destination,
                        actor,
                        available,
                        transform: None,
                    },
                )?;
                Ok(None)
            }
        }
    }

    pub(super) fn allocate_token_requests(
        &mut self,
        requests: &[(String, u64)],
        destination: ZoneKind,
        actor: PlayerId,
        limit: Option<usize>,
    ) -> Result<TokenAllocation> {
        let seat = seat_name(actor)?;
        let zone = if destination == ZoneKind::Field {
            "field"
        } else {
            "ex"
        };
        let space = self.token_capacity(seat, zone)?;
        let available = limit.map_or(space, |limit| space.min(limit));
        let mut requested = Vec::new();
        for (name, count) in requests {
            let card = self.catalog.token_print(name)?;
            check_template(self, name, destination)?;
            if !self.card_runtime.is_loaded(&card) {
                return Err(unsupported("created token requires an admitted whole card"));
            }
            requested.extend(vec![
                card;
                usize::try_from((*count).min(5)).map_err(invalid)?
            ]);
        }
        let prints = self.unique_crest_creations(&requested, zone, seat)?;
        let selected = if prints.len() > available {
            let names = prints
                .iter()
                .map(|card| self.token_name(card))
                .collect::<Result<Vec<_>>>()?;
            let choices = token_combinations(&names, available)?;
            if choices.len() > 1 {
                return Ok(TokenAllocation::Capacity {
                    prints,
                    names,
                    choices,
                    available,
                });
            }
            let chosen = choices
                .first()
                .ok_or_else(|| invalid("missing token capacity result"))?;
            prints_for_names(&prints, &names, chosen)?
        } else {
            prints
        };
        self.internal_create_prints(&selected, seat)
            .map(TokenAllocation::Created)
    }

    fn commit_tokens(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        destination: ZoneKind,
        ids: Vec<String>,
        retry: Node,
    ) -> Result<()> {
        execution.ids = self.internal_identities()?;
        if destination == ZoneKind::Field {
            for id in &ids {
                if self.internal_ward_entry_choice(execution, id)? {
                    execution.creating = Some(Creation {
                        receipt: instruction.receipt,
                        ids,
                    });
                    execution.todo.push_front(retry);
                    return Ok(());
                }
            }
        }
        let stack = match &instruction.action {
            Action::Token {
                initial_state:
                    Some(TokenInitialState::StackCounters {
                        amount: Expr::Literal(amount),
                    }),
                ..
            } => Some(*amount),
            _ => None,
        };
        let created = self.commit_token_objects_with_stack(execution, destination, &ids, stack)?;
        let Action::Token { output, .. } = &instruction.action else {
            return Err(invalid("token output"));
        };
        execution
            .slots
            .insert(output.key(), Slot::Created(created.clone()));
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Token(TokenOutcome { created }),
        );
        Ok(())
    }

    pub(super) fn commit_token_objects(
        &mut self,
        execution: &mut Execution,
        destination: ZoneKind,
        ids: &[String],
    ) -> Result<Vec<ObjectRef>> {
        self.commit_token_objects_with_stack(execution, destination, ids, None)
    }

    fn commit_token_objects_with_stack(
        &mut self,
        execution: &mut Execution,
        destination: ZoneKind,
        ids: &[String],
        stack: Option<Nat>,
    ) -> Result<Vec<ObjectRef>> {
        execution.ids = self.internal_identities()?;
        let before = ids
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        let zone = match destination {
            ZoneKind::Field => "field",
            ZoneKind::Ex => "ex",
            ZoneKind::Equipment => "equipment",
            _ => return Err(unsupported("token destination")),
        };
        let mut frame = execution.frame();
        if let Some(amount) = stack {
            frame.values.insert(
                "stack_entry_counts".into(),
                serde_json::Value::Object(
                    ids.iter()
                        .map(|id| (id.clone(), serde_json::json!(amount.0)))
                        .collect(),
                ),
            );
        }
        self.move_objects(ids, zone, None, None, &frame)?;
        if destination == ZoneKind::Field {
            for member in before {
                self.internal_ward_entry_posture(execution, member)?;
            }
        }
        execution.ids = self.internal_identities()?;
        let created = ids
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        Ok(created)
    }
}

#[derive(Default)]
pub(super) struct Paths {
    stages: Vec<String>,
    counts: Vec<String>,
    names: Vec<String>,
    kinds: Vec<String>,
}

pub(super) fn paths(program: &Program, x: Option<PaidX>) -> Paths {
    let mut accepted = Paths::default();
    collect_paths(program, "", &mut accepted, x);
    accepted
}

fn collect_paths(program: &Program, prefix: &str, accepted: &mut Paths, x: Option<PaidX>) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{prefix}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => {
                if let Some((tokens, _)) = specification_with_x(&instruction.action, x) {
                    for (token_index, token) in tokens.iter().enumerate() {
                        if matches!(
                            &token.template,
                            TokenTemplate::Name {
                                kind: Some(CardKind::Follower),
                                ..
                            }
                        ) {
                            accepted.kinds.push(format!(
                                "{path}/instruction/action/tokens/{token_index}/template/kind"
                            ));
                        }
                        if matches!(&token.template, TokenTemplate::Name { name, .. } if departure_name(name))
                        {
                            accepted.names.push(format!(
                                "{path}/instruction/action/tokens/{token_index}/template/name"
                            ));
                        }
                        if let Ok(plan) =
                            CountPlan::compile_with_x(&token.count, x.map(|paid| paid.0))
                        {
                            accepted.counts.extend(plan.before_paths(&format!(
                                "{path}/instruction/action/tokens/{token_index}/count"
                            )));
                        }
                    }
                }
            }
            Node::Control {
                control: control @ Control::EachPlayer { stages, .. },
            } if check_each(control).is_ok() => {
                accepted.stages.push(format!("{path}/control"));
                for (stage_index, stage) in stages.iter().enumerate() {
                    collect_paths(
                        &stage.body,
                        &format!("{path}/control/stages/{stage_index}/body"),
                        accepted,
                        None,
                    );
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for (mode_index, mode) in modes.iter().enumerate() {
                    collect_paths(
                        &mode.body,
                        &format!("{path}/control/modes/{mode_index}/body"),
                        accepted,
                        None,
                    );
                }
            }
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
            } => collect_paths(body, &format!("{path}/control/body"), accepted, x),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                collect_paths(then_body, &format!("{path}/control/then_body"), accepted, x);
                if let Some(body) = else_body {
                    collect_paths(body, &format!("{path}/control/else_body"), accepted, x);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn requirement(requirement: &Requirement, paths: &Paths) -> bool {
    let (allowed, more): (&[String], &[String]) = match requirement.capability.code {
        "shape.Control.EachPlayer"
        | "control.each-player"
        | "shape.PlayerStage"
        | "internal.StageMode.NonChoiceSimultaneous"
        | "shape.StageMode.NonChoiceSimultaneous"
        | "internal.StageMode.ChoiceProcedure"
        | "shape.StageMode.ChoiceProcedure"
        | "internal.PlayerOrder.TurnPlayerFirst"
        | "shape.PlayerRef.Bound" => (&paths.stages, &[]),
        "shape.ObjectView.LastKnown" | "reference.lki" | "internal.LkiReason.ZoneExit" => {
            (&paths.counts, &paths.names)
        }
        "internal.CardKind.Follower" => (&paths.kinds, &[]),
        "selector.filter"
        | "shape.Selector.Filter"
        | "shape.Predicate.PlayerEquals"
        | "predicate.player-equals" => (&paths.counts, &[]),
        _ => return false,
    };
    requirement.locations.iter().all(|location| {
        allowed.iter().chain(more).any(|prefix| {
            location.ir_path == *prefix
                || location
                    .ir_path
                    .strip_prefix(prefix)
                    .is_some_and(|tail| tail.starts_with('/'))
        })
    })
}

pub(super) fn mask_operands(node: &mut Node, x: Option<PaidX>) {
    let valid = matches!(node, Node::Instruction { instruction } if specification_with_x(&instruction.action, x).is_some());
    if let Node::Instruction { instruction } = node
        && let Action::Token { tokens, .. } = &mut instruction.action
    {
        for token in tokens {
            if valid && let TokenTemplate::Name { kind, .. } = &mut token.template {
                // A validated literal constrains the new template; it observes no private card.
                *kind = None;
            }
            if valid {
                CountPlan::mask_before(&mut token.count, x.map(|paid| paid.0));
            }
        }
    }
}
