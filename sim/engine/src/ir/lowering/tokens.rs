use super::{
    Action, Binding, BodyContext, Expr, LoweringError, Nat, PlayerRef, ZoneKind, ast, unsupported,
};
use crate::ir::events::subject::SubjectFields;
use crate::ir::perspective::performer;
use crate::ir::{
    AbilitySource, CardKind, Cardinality, Control, LkiReason, MemberProjection, MovementReceipt,
    NameOperation, NatOperation, Node, ObjectOperand, ObjectReceipt, ObjectRef, ObjectView,
    PlayerOrder, PlayerRead, PlayerRelation, PlayerSet, PlayerStage, Posture, Predicate, Program,
    RepeatBody, Role, Selector, SourceSpan, StageMode, TextQuantifier, TokenDestination, TokenSpec,
    TokenTemplate, TokenTemplateId, ZoneRef,
};
use alloc::collections::BTreeSet;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Token creation shares the body binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn create(
        &mut self,
        value: &ast::Create,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let kind = match value.to {
            ast::CreateZone::Ex => ZoneKind::Ex,
            ast::CreateZone::Field => ZoneKind::Field,
        };
        if value.result.is_some() && value.bind.is_some() {
            return Err(unsupported(path));
        }
        let output = Binding::new(self.allocator.id(path)?, self.scope);
        if let Some(name) = value.result.as_ref().or(value.bind.as_ref())
            && (self.searches.contains(name)
                || self.observations.contains_key(name)
                || self.targets.insert(name.clone(), output).is_some())
        {
            return Err(unsupported(path));
        }
        if kind == ZoneKind::Ex
            && let Some(name) = value.result.as_ref().or(value.bind.as_ref())
        {
            self.ex_outputs
                .insert(name.clone(), Selector::Bound { reference: output });
        }
        let tokens = if let ast::CreateToken::SameName {
            same_name_as,
            kind: created_kind,
        } = &value.token
        {
            let created_kind = match created_kind.as_deref() {
                None => None,
                Some("follower") => Some(CardKind::Follower),
                Some(_) => return Err(unsupported(path)),
            };
            if !matches!(value.n, None | Some(ast::TokenCount::Fixed(1)))
                || value.by.as_deref().is_some_and(|who| who != "self")
            {
                return Err(unsupported(path));
            }
            let name = if same_name_as == "it" {
                let Some(SubjectFields::Leave(event)) = self.subject_event else {
                    return Err(unsupported(path));
                };
                Expr::Compute(Box::new(NameOperation::UniqueName {
                    object: ObjectView::LastKnown {
                        object: Box::new(ObjectOperand::FromExpression {
                            value: Box::new(SubjectFields::Leave(event).object()),
                        }),
                        reason: LkiReason::ZoneExit,
                    },
                }))
            } else {
                self.selected_name(same_name_as, path)?
            };
            vec![TokenSpec {
                template: TokenTemplate::Name {
                    name,
                    kind: created_kind,
                },
                count: Expr::Literal(Nat(1)),
            }]
        } else {
            let names = value.token.names().ok_or_else(|| unsupported(path))?;
            if names.is_empty() || names.iter().any(String::is_empty) {
                return Err(unsupported(path));
            }
            names
                .iter()
                .map(|name| {
                    Ok(TokenSpec {
                        template: TokenTemplate::Fixed {
                            template: TokenTemplateId(name.clone()),
                        },
                        count: self.token_count(value.n.as_ref(), path)?,
                    })
                })
                .collect::<Result<Vec<_>, LoweringError>>()?
        };
        Ok(Action::Token {
            tokens,
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: self.token_player(value.by.as_deref().unwrap_or("self"), path)?,
                    kind,
                }),
            },
            output,
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        })
    }
    fn token_player(&self, name: &str, path: &str) -> Result<PlayerRef, LoweringError> {
        if name == "self" {
            return Ok(PlayerRef::Controller {});
        }
        if name == "opp" {
            return Ok(super::super::events::timing::opponent());
        }
        self.players
            .get(name)
            .copied()
            .map(|binding| PlayerRef::Bound { binding })
            .ok_or_else(|| unsupported(path))
    }

    fn token_count(
        &mut self,
        value: Option<&ast::TokenCount>,
        path: &str,
    ) -> Result<Expr<Nat>, LoweringError> {
        let count = match value {
            None => return Ok(Expr::Literal(Nat(1))),
            Some(ast::TokenCount::Fixed(count)) => return Ok(Expr::Literal(Nat(*count))),
            Some(ast::TokenCount::Query { count }) => {
                return Ok(Expr::Compute(Box::new(NatOperation::CheckedInt {
                    value: self.zone_count(count, path)?,
                })));
            }
            Some(ast::TokenCount::Number(number)) => {
                return Ok(Expr::Compute(Box::new(NatOperation::CheckedInt {
                    value: self.number(number, path, &mut BTreeSet::new())?,
                })));
            }
            Some(ast::TokenCount::Receipt { count }) => count,
        };
        let ast::BeforeOnly::Before = count.r#in.view;
        let ast::EqualOnly::Eq = count.r#where.player_cmp.op;
        if self.players.contains_key(&count.binding)
            || count.r#where.player_cmp.left.player.master_of != count.binding
        {
            return Err(unsupported(path));
        }
        let receipt = self
            .observations
            .get(&count.r#in.objects_of)
            .filter(|receipt| {
                matches!(
                    receipt,
                    ObjectReceipt::Movement {
                        receipt: MovementReceipt::Destroy { .. }
                    }
                )
            })
            .cloned()
            .ok_or_else(|| unsupported(path))?;
        let binding: Binding<ObjectRef> = Binding::new(self.allocator.id(path)?, self.scope);
        let player = self.token_player(&count.r#where.player_cmp.right.player, path)?;
        Ok(Expr::Compute(Box::new(NatOperation::Count {
            objects: Selector::Filter {
                input: Box::new(Selector::ReceiptMembers {
                    receipt,
                    projection: MemberProjection::Before,
                }),
                binding,
                predicate: Box::new(Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Live(Box::new(PlayerRead::Role {
                        object: ObjectView::LastKnown {
                            object: Box::new(ObjectOperand::Bound { binding }),
                            reason: LkiReason::ZoneExit,
                        },
                        role: Role::Controller,
                    })),
                    right: Expr::Literal(player),
                }))),
            },
        })))
    }

    /// Draws, shuffles and whole returns of the bound player; a draw may count that player's own
    /// share of a return made in an earlier stage (「戻した枚数と同じだけ引く」).
    fn player_procedure(each: &ast::EachPlayer) -> bool {
        let bound = |name: &String| name == &each.binding;
        let mut returned = BTreeSet::new();
        if each.stages.is_empty() {
            return false;
        }
        for stage in &each.stages {
            if stage.is_empty() {
                return false;
            }
            for step in stage {
                let valid = if let ast::Step::Draw(draw) = step {
                    matches!(&draw.who, Some(ast::LifeHolder::Bound(name)) if bound(name))
                        && (matches!(draw.n, ast::Number::Literal(0..=10_000))
                            || matches!(&draw.n, ast::Number::CountOf {
                                count_of: ast::CountOf::Named(name),
                            } if returned.contains(name)))
                } else if let ast::Step::Shuffle(shuffle) = step {
                    matches!(&shuffle.who, ast::DeckHolder::Bound(name) if bound(name))
                } else if let ast::Step::Move(movement) = step {
                    // Every player finishes this return before the next instruction reads it.
                    returned.extend(movement.result.clone());
                    matches!(&movement.r#ref, Some(ast::MoveObjects::Whole(whole))
                        if matches!(&whole.who, ast::DeckHolder::Bound(name) if bound(name)))
                        && Self::unordered_deck(movement)
                } else {
                    false
                };
                if !valid {
                    return false;
                }
            }
        }
        true
    }

    pub(super) fn each_player(
        &mut self,
        each: &ast::EachPlayer,
        path: &str,
        span: &SourceSpan,
        source: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        let legacy = match each.stages.as_slice() {
            [stage] => match stage.as_slice() {
                [ast::Step::Create(create)] => {
                    create.by.as_ref() == Some(&each.binding)
                        && create.result.is_none()
                        && create.bind.is_none()
                }
                [ast::Step::Banish(ast::BanishAction::Top { top })] => {
                    matches!(&top.who, ast::DeckHolder::Bound(name) if name == &each.binding)
                }
                _ => false,
            },
            _ => false,
        };
        let procedure = Self::player_procedure(each);
        // Other per-player bodies run as a choice procedure: one player finishes a stage before
        // the next player starts it, turn player first.
        let choices = !legacy
            && !procedure
            && !each.stages.is_empty()
            && each.stages.iter().all(|stage| !stage.is_empty());
        if matches!(
            each.binding.as_str(),
            "self" | "opp" | "turn" | "non_turn" | "current" | "item"
        ) || self.players.contains_key(&each.binding)
            || self.targets.contains_key(&each.binding)
            || self.observations.contains_key(&each.binding)
            || self.searches.contains(&each.binding)
            || !(legacy || procedure || choices)
        {
            return Err(unsupported(path));
        }
        let parent = self.scope;
        // Stage results and choices are local to this procedure, like the player binding.
        let saved = self.local_bindings();
        self.scope = self.card_allocator.scope(path)?;
        let player = Binding::new(self.allocator.id(path)?, self.scope);
        self.players.insert(each.binding.clone(), player);
        let outer = self.procedure_player.replace(player);
        let mut stages = Vec::new();
        let result: Result<(), LoweringError> = (|| {
            for (index, stage) in each.stages.iter().enumerate() {
                let stage_path = format!("{path}/each_player/stages/{index}");
                self.scope = self.card_allocator.scope(&stage_path)?;
                let mut body = self.steps(stage, &stage_path, span, source)?;
                let stage_player = PlayerRef::Bound { binding: player };
                if choices {
                    performed_by_procedure_player(&mut body);
                } else {
                    for step in &mut body.steps {
                        if let Node::Instruction { instruction } = &mut step.node {
                            instruction.actor = stage_player.clone();
                        }
                    }
                }
                stages.push(PlayerStage {
                    body,
                    mode: if choices {
                        StageMode::ChoiceProcedure {}
                    } else {
                        StageMode::NonChoiceSimultaneous {}
                    },
                });
            }
            Ok(())
        })();
        self.players.remove(&each.binding);
        self.procedure_player = outer;
        self.restore_bindings(saved);
        self.scope = parent;
        result?;
        Ok(Node::Control {
            control: Control::EachPlayer {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                },
                player,
                stages,
                choice_order: PlayerOrder::TurnPlayerFirst,
            },
        })
    }
}

/// In a choice stage an action keeps the actor its own lowering gave it, such as the effect
/// controller for `win who: self`; an action whose performer is a procedure player in scope
/// (`draw who: p`, or an outer `shuffle who: p` inside a nested procedure) is performed by
/// that player.
fn performed_by_procedure_player(program: &mut Program) {
    for step in &mut program.steps {
        if let Node::Instruction { instruction } = &mut step.node
            && let Some(player @ PlayerRef::Bound { .. }) = performer(&instruction.action)
        {
            instruction.actor = player.clone();
        }
        if let Node::Control { control } = &mut step.node {
            if let Control::Sequence { body }
            | Control::May { body, .. }
            | Control::EachObject { body, .. }
            | Control::Pay { on_paid: body, .. }
            | Control::Repeat {
                body: RepeatBody::Program { body },
                ..
            } = control
            {
                performed_by_procedure_player(body);
            }
            if let Control::If {
                then_body,
                else_body,
                ..
            } = control
            {
                performed_by_procedure_player(then_body);
                if let Some(body) = else_body {
                    performed_by_procedure_player(body);
                }
            }
            if let Control::Mode { modes, .. } = control {
                for mode in modes {
                    performed_by_procedure_player(&mut mode.body);
                }
            }
        }
    }
}
