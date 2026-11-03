mod capacity;
mod dynamic;
pub(super) mod equipment;

use crate::ir::{KeywordId, ObjectRef, PlayerId, printed_keywords};
use alloc::sync::Arc;

pub(super) use capacity::CapacityWait;

pub(super) use dynamic::check_each;

use super::{Execution, Game, InstructionOutcome, Result, Slot, invalid, seat_name, unsupported};
use crate::game::tokens::{prints_for_names, token_combinations};
use crate::ir::events::timing::opponent;
use crate::ir::{
    Action, CardName, Control, Expr, Instruction, LkiReason, Located, NameOperation, Nat, Node,
    ObjectFrozen, ObjectOperand, ObjectView, PlayerRef, Posture, Program, ReceiptId, Requirement,
    TokenDestination, TokenInitialState, TokenOutcome, TokenSpec, TokenTemplate, ZoneKind, ZoneRef,
};

#[derive(Clone, Debug)]
pub(super) struct Creation {
    receipt: ReceiptId,
    ids: Vec<String>,
}

fn specification(action: &Action) -> Option<(&[TokenSpec], &ZoneRef)> {
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
                matches!(token.count, Expr::Literal(_))
                    || dynamic::projection(&token.count).is_some()
                    || dynamic::zone_count(&token.count).is_some()
            }
            TokenTemplate::Name { name } => {
                tokens.len() == 1
                    && token.count == Expr::Literal(Nat(1))
                    && zone.player == (PlayerRef::Controller {})
                    && initial_state.is_none()
                    && (matches!(name, Expr::Captured(_)) || departure_name(name))
            }
        })
        .then_some((tokens, zone))
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

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::Token { .. }) && specification(action).is_none() {
        return Err(unsupported(
            "token creation with admitted counts and destination",
        ));
    }
    Ok(())
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Some((tokens, zone)) = specification(action) {
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
                    .map(Arc::as_ref)
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
        let (_, destination) = specification(&instruction.action)
            .ok_or_else(|| unsupported("token creation shape"))?;
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

    pub(super) fn internal_player_tokens(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        check_each(control)?;
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
        let (_, destination) = specification(&instruction.action)
            .ok_or_else(|| unsupported("token creation shape"))?;
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
        let (tokens, destination) = specification(&instruction.action)
            .ok_or_else(|| unsupported("token creation shape"))?;
        let actor = execution.player(self, &destination.player)?;
        self.allocate_token_specs(execution, instruction, tokens, destination.kind, actor)
    }

    pub(super) fn allocate_token_specs(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        tokens: &[TokenSpec],
        destination: ZoneKind,
        actor: PlayerId,
    ) -> Result<Option<Vec<String>>> {
        let seat = seat_name(actor)?;
        let zone = if destination == ZoneKind::Field {
            "field"
        } else {
            "ex"
        };
        let available = self.token_capacity(seat, zone)?;
        let mut requested = Vec::new();
        for token in tokens {
            let count = dynamic::count(execution, self, &token.count)?;
            let name = match &token.template {
                TokenTemplate::Fixed { template } => template.0.clone(),
                TokenTemplate::Name { name } => execution.name(self, name)?.0,
            };
            check_template(self, &name, destination)?;
            let card = self.catalog.token_print(&name)?;
            if !self.card_runtime.is_loaded(&card) {
                return Err(unsupported("created token requires an admitted whole card"));
            }
            // At most five identical names can contribute to any capacity-limited combination.
            requested.extend(vec![card; usize::try_from(count.min(5)).map_err(invalid)?]);
        }
        let prints = self.unique_crest_creations(&requested, zone, seat)?;
        let selected = if prints.len() > available {
            let names = prints
                .iter()
                .map(|card| self.token_name(card))
                .collect::<Result<Vec<_>>>()?;
            let choices = token_combinations(&names, available)?;
            if choices.len() > 1 {
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
                    },
                )?;
                return Ok(None);
            }
            let chosen = choices
                .first()
                .ok_or_else(|| invalid("missing token capacity result"))?;
            prints_for_names(&prints, &names, chosen)?
        } else {
            prints
        };
        self.internal_create_prints(&selected, seat).map(Some)
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
}

pub(super) fn paths(program: &Program) -> Paths {
    let mut accepted = Paths::default();
    collect_paths(program, "", &mut accepted);
    accepted
}

fn collect_paths(program: &Program, prefix: &str, accepted: &mut Paths) {
    for (index, step) in program.steps.iter().enumerate() {
        let path = format!("{prefix}/steps/{index}");
        match &step.node {
            Node::Instruction { instruction } => {
                if let Some((tokens, _)) = specification(&instruction.action) {
                    for (token_index, token) in tokens.iter().enumerate() {
                        if matches!(&token.template, TokenTemplate::Name { name } if departure_name(name))
                        {
                            accepted.names.push(format!(
                                "{path}/instruction/action/tokens/{token_index}/template/name"
                            ));
                        }
                        if dynamic::projection(&token.count).is_some() {
                            accepted.counts.push(format!(
                                "{path}/instruction/action/tokens/{token_index}/count"
                            ));
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
                    );
                }
            }
            Node::Control {
                control: Control::Sequence { body } | Control::May { body, .. },
            } => collect_paths(body, &format!("{path}/control/body"), accepted),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                collect_paths(then_body, &format!("{path}/control/then_body"), accepted);
                if let Some(body) = else_body {
                    collect_paths(body, &format!("{path}/control/else_body"), accepted);
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
        | "internal.PlayerOrder.TurnPlayerFirst"
        | "shape.PlayerRef.Bound" => (&paths.stages, &[]),
        "shape.ObjectView.LastKnown" | "reference.lki" | "internal.LkiReason.ZoneExit" => {
            (&paths.counts, &paths.names)
        }
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

pub(super) fn mask_count(node: &mut Node) {
    if let Node::Instruction { instruction } = node
        && let Action::Token { tokens, .. } = &mut instruction.action
    {
        for token in tokens {
            if dynamic::projection(&token.count).is_some() {
                token.count = Expr::Literal(Nat(0));
            }
        }
    }
}
